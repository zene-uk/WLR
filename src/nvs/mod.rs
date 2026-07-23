pub mod init;
mod record_paging;
mod data_paging;
mod common;
mod page_address;

use core::{marker::PhantomData, mem::MaybeUninit};
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsError::{self, InconsistentSize}, NvsKey, Padding, data::{Address, Record}, key_map::KeyMap, map_err, nvs::page_address::PageAddresses, round_up, state::State};

pub struct Nvs<K: NvsKey, T: NorFlash, C: NvsConstants>
{
    partition: T,
    key_map: KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
    /// the next addresses for data and records
    page_address: PageAddresses<{ C::PAGE_SIZE }>,
    state: State<T, C>,
    _phantom: PhantomData<C>
}
struct NvsShadow<'a, K: NvsKey, T: NorFlash, C: NvsConstants, F: Fn(K) -> bool>
{
    partition: &'a mut T,
    key_map: &'a mut KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
    /// the next addresses for data and records
    page_address: &'a mut PageAddresses<{ C::PAGE_SIZE }>,
    state: &'a mut State<T, C>,
    ignore: F,
    _phantom: PhantomData<C>
}
impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    const RECORD_OFFSET: usize = round_up!(size_of::<Record<{ C::PAGE_SIZE }>>(), C::WRITE_SIZE);
    
    pub fn new(partition: &'a mut T,
        key_map: &'a mut KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
        page_address: &'a mut PageAddresses<{ C::PAGE_SIZE }>,
        state: &'a mut State<T, C>,
        ignore: F) -> NvsShadow<'a, K, T, C, F>
    {
        return NvsShadow { partition, key_map, page_address, state, ignore, _phantom: PhantomData };
    }
}

impl<K: NvsKey, T: NorFlash, C: NvsConstants + 'static> Nvs<K, T, C>
{
    fn as_shadow<'a, F: Fn(K) -> bool>(&'a mut self, ignore: F) -> NvsShadow<'a, K, T, C, F>
    {
        return NvsShadow::new(&mut self.partition, &mut self.key_map, &mut self.page_address, &mut self.state, ignore);
    }
    
    pub fn write_key_value<V: bytemuck::Pod>(&mut self, key: K, value: &V) -> Result<(), NvsError<K, T>>
        where V: PartialEq,
    {
        let mut tmp: V = unsafe { MaybeUninit::zeroed().assume_init() };
        self.read_key_value(key, &mut tmp)?;
        if value == &tmp
        {
            return Ok(());
        }
        
        return self.write_key_value_force(key, value);
    }
    /// Does not check whether the data has changed or not
    pub fn write_key_value_force<V: bytemuck::Pod>(&mut self, key: K, value: &V) -> Result<(), NvsError<K, T>>
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
        
        // data cannot be bigger than a page
        if size_of::<V>() > C::PAGE_SIZE as usize
        {
            return Err(NvsError::DataTooBig(size_of::<V>()))
        }
        
        let mut shadow = self.as_shadow(|k| k == key);
        // prepare next_record_address
        shadow.prepare_map()?;
        let data_addr;
        
        // actually write the data - this may change next_record_address
        // out is already aligned by WRITE_SIZE
        if size_of::<V>() % C::WRITE_SIZE == 0
        {
            data_addr = shadow.write_entry_data(bytemuck::bytes_of(value), &[])?;
        }
        // otherwise reallocate with extra space for alignment
        else
        {
            let mut v: Padding<V, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
            v.0 = *value;
            // round up to WRITE_SIZE
            let size = round_up!(size_of::<V>(), C::WRITE_SIZE);
            
            data_addr = shadow.write_entry_data(v.as_bytes(size), &[])?;
        }
        
        let size = size_of::<V>() as u16;
        // write and update the record
        match shadow.key_map.get_table_value(key)
        {
            Some(mut tv) =>
            {
                tv.set_size(size);
                let rec_addr = NvsShadow::<'_, K, T, C, fn(K) -> bool>::write_record(&mut self.partition,
                    &self.state, &mut self.page_address.record, &tv, data_addr)?;
                let record = tv.to_record_new_addr(data_addr);
                if self.key_map.update_record(record, rec_addr).is_none()
                {
                    return Err(NvsError::MissingKey(key));
                }
            },
            None =>
            {
                let record = Record { size, key: key.get_key_value(), address: data_addr };
                let rec_addr = shadow.write_new_record(record)?;
                if !self.key_map.add_value_page(record, rec_addr)
                {
                    return Err(NvsError::DuplicateKey(key));
                }
            }
        }
        
        // do that at the very end so that our record update
        // doesnt clear its old potentially invalid location 
        self.state.shift_tmp_to_value();
        
        return Ok(());
    }
    
    /// Call after every block of writes
    #[inline]
    pub fn flush(&mut self) -> Result<(), NvsError<K, T>>
    {
        // rewrite current data page
        if self.page_address.update_address_record
        {
            let mut shadow = self.as_shadow(|_| false);
            // retain this order
            shadow.prepare_map()?;
            // this is only called to make sure the value prepare_map
            // left in next_data_address is in the partition
            shadow.prepare_data_page(0)?;
            shadow.write_new_record(Record { size: 0xFFFF, key: 0x0000, address: Address(shadow.page_address.get_data_page()) })?;
            self.page_address.update_address_record = false;
        }
        
        // write state value
        return map_err!{self.state.sync_value(&mut self.partition)};
    }
    
    #[must_use]
    pub fn read_key_value_direct<V: bytemuck::Pod>(&mut self, key: K) -> Result<V, NvsError<K, T>>
    {
        let mut result: V = unsafe { MaybeUninit::zeroed().assume_init() };
        return self.read_key_value(key, &mut result).map(|_| result);
    }
    pub fn read_key_value<V: bytemuck::Pod>(&mut self, key: K, out: &mut V) -> Result<(), NvsError<K, T>>
    {
        let tv = match self.key_map.get_table_value(key)
        {
            Some(tv) => tv,
            None => return Err(NvsError::MissingKey(key))
        };
        
        // tv.get_size() <= C::PAGE_SIZE so not a concern
        if tv.get_size() as usize != size_of::<V>()// || size_of::<V>() > C::PAGE_SIZE
        {
            return Err(InconsistentSize(tv.get_size()));
        }
        
        // out is already aligned by READ_SIZE
        if size_of::<V>() % C::READ_SIZE == 0
        {
            map_err!{self.partition.read(tv.get_address().0, bytemuck::bytes_of_mut(out))}?;
        }
        // otherwise reallocate with extra space for alignment
        else
        {
            let mut v: Padding<V, { C::READ_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
            // round up to READ_SIZE
            let size = round_up!(size_of::<V>(), C::READ_SIZE);
            
            map_err!{self.partition.read(tv.get_address().0, v.as_bytes_mut(size))}?;
            
            *out = v.0;
        }
        
        return Ok(());
    }
}
