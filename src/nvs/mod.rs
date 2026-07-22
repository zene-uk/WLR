pub mod init;
pub mod paging;
mod common;

use core::{marker::PhantomData, mem::MaybeUninit};

use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, Padding, data::Address, key_map::KeyMap, paging::NvsShadow, state::State};
// use crate::{CheckConst, True};

pub struct Nvs<K: NvsKey, T: NorFlash, C: NvsConstants>
    where //CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        // CheckConst<{ K::COUNT < 0xFFFF }>: True,
        [(); T::WRITE_SIZE]: ,
        [(); T::READ_SIZE]: ,
        [(); { T::ERASE_SIZE as u32 } as usize]: ,
        [(); K::COUNT]: 
{
    partition: T,
    key_map: KeyMap<K, { T::ERASE_SIZE as u32 }, { T::WRITE_SIZE }>,
    next_data_address: Address<{ T::ERASE_SIZE as u32 }>,
    next_record_address: Address<{ T::ERASE_SIZE as u32 }>,
    state: State<C, { T::ERASE_SIZE as u32 }>,
    _phantom: PhantomData<C>
}

impl<K: NvsKey, T: NorFlash + 'static, C: NvsConstants + 'static> Nvs<K, T, C>
    where //CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        // CheckConst<{ K::COUNT < 0xFFFF }>: True,
        [(); T::WRITE_SIZE]: ,
        [(); T::READ_SIZE]: ,
        [(); { T::ERASE_SIZE as u32 } as usize]: ,
        [(); K::COUNT]: 
{
    pub fn as_shadow<'a>(&'a mut self) -> NvsShadow<'a, K, T, C>
    {
        return NvsShadow::new(&mut self.partition, &mut self.key_map, &mut self.next_data_address, &mut self.next_record_address, &mut self.state);
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
    /// Does not check whether the data has changed or not
    pub fn write_key_value_force<V: bytemuck::Pod>(&mut self, key: K, value: &V)
    {
        // order of operations:
        // check whether next record is on new page
        // check whether we need to move some items - ignore the item we are about to change - read entire page into memory so that we can write new records
        // update their records first, (no need to clear old values if they are on a page about to be moved)
        //      also the number of items moved to make way for a new page will be less than a full page of records
        //      may need to move more if the data pages we are writing to need reorganising
        //      calculate all new records first - then start moving data
        // then move back records to front if the mapping range is to long - ignore the item we are about to change (and data page if applicable)
        // (repeat until we can add our new record)
        // update state if needed
        // check whether the next data address is on a new page - find next page if needed
        // check whether our data can fit on the page - find next page if needed
        // update data page record if needed
        // add data
        // add new record
        
        // on update_records:
        // if next address is on new page:
        //      if page contains data:
        //          move those items
        //          clear up to MAP_POST_PADDING
        //      if map range is now too big
        //          move back records to front to clear page
        //          change state
        // add new record
        
        // on get_next_page:
        // increment page counter
        // skip over mapping region
        // if page contains contents:
        //      page is too full if we cant add at least 2x the data we want to
        //      read data - entire page, then use record data into that memory region
        //      clear page
        //      write back old data
        // if page is too full, increment to next page again
        // throw error if we have filled up our allowed space
        
        self.as_shadow().prepare_map(key);
    }
    
    /// Call after every block of writes
    #[inline]
    pub fn flush_state(&mut self) -> bool
    {
        return self.state.sync_value(&mut self.partition);
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
