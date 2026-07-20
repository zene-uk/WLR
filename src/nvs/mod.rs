pub mod init;
pub mod paging;

use core::{marker::PhantomData, mem::MaybeUninit};

use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, NvsConstants, NvsKey, Padding, True, data::Address, key_map::KeyMap, state::State};

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
