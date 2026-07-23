use core::{mem::MaybeUninit, slice};

use embedded_storage::nor_flash::NorFlash;
use hashbrown::HashMap;

use crate::{Ignore, IgnoreTy, Nvs, NvsConstants, NvsError, NvsKey, Padding, WriteQueue, cache::PageData, data::{Address, Record}, key_map::KeyMap, map_err, nvs::NvsShadow, round_up};

impl<K: NvsKey, T: NorFlash, C: NvsConstants + 'static, const KEY_COUNT: usize> Nvs<K, T, C, KEY_COUNT>
    where [(); C::WRITE_SIZE]: ,
        [(); C::READ_SIZE]:
{
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
    #[inline]
    pub fn write_key_values<V: bytemuck::Pod + PartialEq>(&mut self, key: K, values: &[V]) -> Result<(), NvsError<K, T>>
    {
        return self.write_key_values_inner(key, values, |k, _, _| k == key);
    }
    pub(crate) fn write_key_values_inner<V: bytemuck::Pod + PartialEq, F: Ignore<K, C, KEY_COUNT>>(
        &mut self, key: K, values: &[V], ignore: F) -> Result<(), NvsError<K, T>>
    {
        let size = size_of::<V>() * values.len();
        // data cannot be bigger than a page
        if size > C::PAGE_SIZE as usize
        {
            return Err(NvsError::DataTooBig(size))
        }
        // use cache as temporary data - it won't be in use at this time
        // data can't be bigger than page size
        let mut bytes = self.cache.get_or_alloc(C::PAGE_SIZE as usize);
        // round up to READ_SIZE
        let align_size = round_up!(size, C::READ_SIZE);
        
        self.read_key_values_inner(key, &mut bytes[..align_size], false)?;
        // do nothing as data hasn't changed
        if values == bytemuck::cast_slice(&bytes[..size])
        {
            self.cache.return_cold(bytes);
            return Ok(());
        }
        
        self.cache.return_cold(bytes);
        return self.write_key_values_force_inner(key, values, ignore);
    }
    #[inline]
    /// Does not check whether the data has changed or not
    pub fn write_key_value_force<V: bytemuck::Pod>(&mut self, key: K, value: &V) -> Result<(), NvsError<K, T>>
    {
        return self.write_key_values_force(key, slice::from_ref(value));
    }
    #[inline]
    /// Does not check whether the data has changed or not
    pub fn write_key_values_force<V: bytemuck::Pod>(&mut self, key: K, values: &[V]) -> Result<(), NvsError<K, T>>
    {
        return self.write_key_values_force_inner(key, values, |k, _, _| k == key);
    }
    /// Does not check whether the data has changed or not
    pub(crate) fn write_key_values_force_inner<V: bytemuck::Pod, F: Ignore<K, C, KEY_COUNT>>(
        &mut self, key: K, values: &[V], ignore: F) -> Result<(), NvsError<K, T>>
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
        
        let size = size_of::<V>() * values.len();
        if size == 0 { return Ok(()); }
        
        // data cannot be bigger than a page
        if size > C::PAGE_SIZE as usize
        {
            return Err(NvsError::DataTooBig(size))
        }
        
        let mut shadow = self.as_shadow(ignore);
        // prepare page_address.record
        shadow.prepare_map()?;
        let data_addr;
        
        // actually write the data - this may change page_address.record
        // out is already aligned by WRITE_SIZE
        if size % C::WRITE_SIZE == 0
        {
            data_addr = shadow.write_entry_data(PageData::Borrowed(bytemuck::cast_slice(values)), PageData::None, false)?;
        }
        // otherwise reallocate with extra space for alignment
        else if values.len() == 1 // more efficient method for only one
        {
            let mut v: Padding<V, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
            v.0 = values[0];
            // round up to WRITE_SIZE
            let size = round_up!(size, C::WRITE_SIZE);
            
            data_addr = shadow.write_entry_data(PageData::Borrowed(v.as_bytes(size)), PageData::None, false)?;
        }
        else
        {
            // use cache as temporary data - it may be needed here but oh well (quite unlikely to cause much overhead)
            // data can't be bigger than page size
            let mut bytes = shadow.cache.get_or_alloc(C::PAGE_SIZE as usize);
            bytes.copy_from_slice(bytemuck::cast_slice(values));
            // round up to WRITE_SIZE
            let align_size = round_up!(size, C::WRITE_SIZE);
            
            data_addr = shadow.write_entry_data(PageData::Borrowed(&bytes[..align_size]), PageData::None, false)?;
            shadow.cache.return_cold(bytes);
        }
        
        // all cached pages are out of date now - the old data exists somewhere else
        shadow.cache.drop_all_pages();
        
        let size = size as u16;
        // write and update the record
        match shadow.key_map.get_table_value(key)
        {
            Some(mut tv) =>
            {
                tv.set_size(size);
                let rec_addr = NvsShadow::<'_, K, T, C, IgnoreTy<K, C, KEY_COUNT>, KEY_COUNT>::write_record(&mut self.partition,
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
            let mut shadow = self.as_shadow(|_, _, _| false);
            // retain this order
            shadow.prepare_map()?;
            // this is only called to make sure the value prepare_map
            // left in page_address.data is in the partition
            shadow.prepare_data_page(0, false)?;
            shadow.write_new_record(Record { size: 0xFFFF, key: 0x0000, address: Address::u(shadow.page_address.get_data_page()) })?;
            self.page_address.update_address_record = false;
        }
        
        // write state value
        return map_err!{self.state.sync_value(&mut self.partition)};
    }
    
    #[inline]
    #[must_use]
    pub fn start_write_queue<'a>(&mut self) -> Result<WriteQueue<'a, K>, NvsError<K, T>>
    {
        return match core::mem::replace(&mut self.write_queue, None)
        {
            Some(wq) => Ok(WriteQueue::new(wq)),
            None => Err(NvsError::AlreadyStartedWriteQueue)
        };
    }
    pub fn end_write_queue(&mut self, wq: WriteQueue<K>) -> Result<(), NvsError<K, T>>
    {
        let mut hash_map = wq.get_back();
        
        for (key, (bytes, force), ignore) in drain_ignore::<_, _, C, KEY_COUNT>(&mut hash_map)
        {
            if force
            {
                self.write_key_values_force_inner(key, bytes, ignore)?;
            }
            else
            {
                self.write_key_values_inner(key, bytes, ignore)?;
            }
        }
        
        hash_map.clear();
        self.write_queue = Some(hash_map);
        
        return Ok(());
    }
}

pub fn drain_ignore<'a, K: NvsKey, V, C: NvsConstants, const KEY_COUNT: usize>(hash_map: &'a mut HashMap<K, (V, bool)>)
    -> impl Iterator<Item = (K, (V, bool), impl Ignore<K, C, KEY_COUNT>)> + 'a
{
    let copy = hash_map as *mut HashMap<K, (V, bool)>;
    return hash_map.drain().map(move |(k, v)|
    {
        (k, v, move |k: K, _: &KeyMap<K, C, KEY_COUNT>, clear: bool| unsafe {
            let hm = copy.as_mut().unwrap_unchecked();
            // if we are clearing, turn this into force because there is no longer any old data to check
            hm.get_mut(&k).map(|v|
            {
                if clear
                {
                    v.1 = true;
                }
            }).is_some()
        })
    });
}

