use core::{marker::PhantomData, mem::MaybeUninit};

use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, NvsConstants, NvsKey, Padding, True, data::{Address, Record}, key_map::KeyMap, round_up, state::State};

pub struct Nvs<K: NvsKey, T: NorFlash, C: NvsConstants>
    where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True,
        [(); T::WRITE_SIZE]: ,
        [(); T::READ_SIZE]: 
{
    partition: T,
    key_map: KeyMap<K, { T::ERASE_SIZE as u32 }, { T::WRITE_SIZE }>,
    next_data_address: Address<{ T::ERASE_SIZE as u32 }>,
    next_record_address: Address<{ T::ERASE_SIZE as u32 }>,
    state: State<C, { T::ERASE_SIZE as u32 }>,
    _phantom: PhantomData<C>
}

impl<K: NvsKey, T: NorFlash + 'static, C: NvsConstants + 'static> Nvs<K, T, C>
    where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True,
        [(); T::WRITE_SIZE]: ,
        [(); T::READ_SIZE]: 
{
    #[must_use]
    pub fn init(mut partition: T) -> Option<Self>
    {
        let state = State::init(&mut partition)?;
        let record_page = state.get_value();
        
        let mut key_map = KeyMap::new();
        let offset = round_up!(size_of::<Record<{ T::ERASE_SIZE as u32 }>>(), T::WRITE_SIZE);
        
        let mut next_data_address = Address(0);
        let mut next_record_address = Address(0);
        
        // find all records
        for page in record_page..(record_page + C::MAPPING_MAX_RANGE as u32)
        {
            let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(T::ERASE_SIZE).assume_init() };
            // read page
            if partition.read(Address::<{ T::ERASE_SIZE as u32 }>::from_page(page as u32).0, &mut bytes).is_err()
            {
                return None;
            }
            
            for i in (0..T::ERASE_SIZE).step_by(offset)
            {
                let key: u32 = *bytemuck::from_bytes(&bytes[i..(i+size_of::<u32>())]);
                match key
                {
                    // stores extra value
                    0xFFFF_0000 =>
                    {
                        // read next u32
                        let value: u32 = *bytemuck::from_bytes(&bytes[(i+size_of::<u32>())..(i+size_of::<u32>()+size_of::<u32>())]);
                        next_data_address = Address(value);
                    },
                    // unset data - no more records
                    0xFFFF_FFFF =>
                    {
                        next_record_address = Address::from_page_offset(page, i as u32);
                    }
                    // empty record
                    0 => continue,
                    // record contains data
                    _ =>
                    {
                        let record: Record<{ T::ERASE_SIZE as u32 }> = 
                            *bytemuck::from_bytes(&bytes[i..(i+size_of::<Record<{ T::ERASE_SIZE as u32 }>>())]);
                        let ra = Address::from_page_offset(page, i as u32);
                        if !key_map.add_value(K::from_key_value(record.key), ra, record.address, record.size)
                        {
                            return None;
                        }
                    }
                }
            }
        }
        
        // create page info
        key_map.initialise();
        
        return Some(Self { partition, key_map, next_data_address, next_record_address, state, _phantom: PhantomData });
    }
    #[must_use]
    pub fn new(mut partition: T) -> Option<Self>
    {
        let mut key_map = KeyMap::new();
        key_map.initialise();
        
        let next_data_address = Address::from_page(C::STATE_PAGES as u32 + C::MAP_POST_PADDING as u32);
        let next_record_address = Address::from_page(C::STATE_PAGES as u32);
        
        // erase initial record page
        if partition.erase(next_data_address.0, next_data_address.0 + T::ERASE_SIZE as u32).is_err()
        {
            return None;
        }
        
        // erase initial data page
        if partition.erase(next_record_address.0, next_record_address.0 + T::ERASE_SIZE as u32).is_err()
        {
            return None;
        }
        
        let state = State::new(&mut partition, 0)?;
        return Some(Self { partition, key_map, next_data_address, next_record_address, state, _phantom: PhantomData })
    }
    
    pub fn write_key_value<V: bytemuck::Pod>(&mut self, key: K, value: &V)
        where V: PartialEq,
    {
        let mut tmp: V = unsafe { MaybeUninit::zeroed().assume_init() };
        if self.read_key_value(key, &mut tmp) && value == &tmp
        {
            return;
        }
        
        self.write_key_value_force(key, value);
    }
    /// does not check whether the data has changed or not
    pub fn write_key_value_force<V: bytemuck::Pod>(&mut self, key: K, value: &V)
    {
        
    }
    
    #[must_use]
    pub fn read_key_value_direct<V: bytemuck::Pod>(&mut self, key: K) -> Option<V>
    {
        let mut result: V = unsafe { MaybeUninit::zeroed().assume_init() };
        if self.read_key_value(key, &mut result)
        {
            return Some(result);
        }
        
        return None;
    }
    pub fn read_key_value<V: bytemuck::Pod>(&mut self, key: K, out: &mut V) -> bool
    {
        let tv = match self.key_map.get_table_value(key)
        {
            Some(tv) => tv,
            None => return false
        };
        
        // tv.get_size() <= T::ERASE_SIZE so not a concern
        if tv.get_size() as usize != size_of::<V>()// || size_of::<V>() > T::ERASE_SIZE
        {
            return false;
        }
        
        // out is already aligned by READ_SIZE
        if size_of::<V>() % T::READ_SIZE == 0
        {
            if self.partition.read(tv.get_address().0, bytemuck::bytes_of_mut(out)).is_err()
            {
                return false;
            }
        }
        // otherwise reallocate with extra space for alignment
        else
        {
            let mut v: Padding<V, { T::READ_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
            let bytes = bytemuck::bytes_of_mut(&mut v);
            // round down to READ_SIZE
            let size = (bytes.len() / T::READ_SIZE) * T::READ_SIZE;
            
            if self.partition.read(tv.get_address().0, &mut bytes[..size]).is_err()
            {
                return false;
            }
            
            *out = v.0;
        }
        
        return true;
    }
}
