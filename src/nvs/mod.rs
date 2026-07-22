pub mod init;
mod record_paging;
mod data_paging;
mod common;

use core::{marker::PhantomData, mem::MaybeUninit};
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, Padding, data::{Address, Record}, key_map::KeyMap, round_up, state::State};

pub struct Nvs<K: NvsKey, T: NorFlash, C: NvsConstants>
{
    partition: T,
    key_map: KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
    next_data_address: Address<{ C::PAGE_SIZE }>,
    next_record_address: Address<{ C::PAGE_SIZE }>,
    state: State<T, C, { C::PAGE_SIZE }>,
    _phantom: PhantomData<C>
}
struct NvsShadow<'a, K: NvsKey, T: NorFlash, C: NvsConstants, F: Fn(K) -> bool>
{
    partition: &'a mut T,
    key_map: &'a mut KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
    next_data_address: &'a mut Address<{ C::PAGE_SIZE }>,
    next_record_address: &'a mut Address<{ C::PAGE_SIZE }>,
    state: &'a mut State<T, C, { C::PAGE_SIZE }>,
    ignore: F,
    _phantom: PhantomData<C>
}
impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    const RECORD_OFFSET: usize = round_up!(size_of::<Record<{ C::PAGE_SIZE }>>(), T::WRITE_SIZE);
    
    pub fn new(partition: &'a mut T,
        key_map: &'a mut KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
        next_data_address: &'a mut Address<{ C::PAGE_SIZE }>,
        next_record_address: &'a mut Address<{ C::PAGE_SIZE }>,
        state: &'a mut State<T, C, { C::PAGE_SIZE }>,
        ignore: F) -> NvsShadow<'a, K, T, C, F>
    {
        return NvsShadow { partition, key_map, next_data_address, next_record_address, state, ignore, _phantom: PhantomData };
    }
}

impl<K: NvsKey, T: NorFlash, C: NvsConstants + 'static> Nvs<K, T, C>
{
    fn as_shadow<'a, F: Fn(K) -> bool>(&'a mut self, ignore: F) -> NvsShadow<'a, K, T, C, F>
    {
        return NvsShadow::new(&mut self.partition, &mut self.key_map, &mut self.next_data_address,
            &mut self.next_record_address, &mut self.state, ignore);
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
        
        let mut shadow = self.as_shadow(|k| k == key);
        shadow.prepare_map();
        
        // out is already aligned by WRITE_SIZE
        if size_of::<V>() % T::WRITE_SIZE == 0
        {
            shadow.write_entry_data(bytemuck::bytes_of(value), &[], 0);
        }
        // otherwise reallocate with extra space for alignment
        else
        {
            let mut v: Padding<V, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
            v.0 = *value;
            // round up to READ_SIZE
            let size = round_up!(size_of::<V>(), T::READ_SIZE);
            
            shadow.write_entry_data(v.as_bytes(size), &[], 0);
        }
        
    }
    
    /// Call after every block of writes
    #[inline]
    pub fn flush(&mut self) -> bool
    {
        // write current data page
        let mut shadow = self.as_shadow(|_| false);
        // retain this order
        if !shadow.prepare_map()
        {
            return false;
        }
        // this is only called to make sure the value prepare_map
        // left in next_data_address is in the partition
        if shadow.prepare_data_page(0, 0).is_none()
        {
            return false;
        }
        if !shadow.write_new_record(Record { size: 0xFFFF, key: 0x0000, address: Address(shadow.next_data_address.get_page()) })
        {
            return false;
        }
        
        // write state value
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
            let mut v: Padding<V, { C::READ_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
            // round up to READ_SIZE
            let size = round_up!(size_of::<V>(), T::READ_SIZE);
            
            if self.partition.read(tv.get_address().0, v.as_bytes_mut(size)).is_err()
            {
                return false;
            }
            
            *out = v.0;
        }
        
        return true;
    }
}
