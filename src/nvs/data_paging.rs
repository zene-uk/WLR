use core::panic;
use embedded_storage::nor_flash::NorFlash;

use crate::{Ignore, NvsConstants, NvsError, NvsKey, cache::{PageCache, PageData}, data::Address, key_map::TableValue, map_err, nvs::NvsShadow};

#[derive(Debug)]
enum PreparePage<K: NvsKey, T: NorFlash, C: NvsConstants>
{
    NextAddress(Address<C>),
    Repeat,
    Fail(NvsError<K, T>)
}

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Ignore<K, C, KEY_COUNT>, const KEY_COUNT: usize> NvsShadow<'a, K, T, C, F, KEY_COUNT>
    where [(); C::WRITE_SIZE]:
{
    pub fn next_data_page(&mut self)
    {
        let mut page = self.page_address.get_data_page() + 1;
        // loop around
        if Self::is_page_overrun(page)
        {
            page = C::STATE_PAGES as u32;
        }
        if !self.page_in_map_padding(page)
        {
            // plus one inside so that it wraps around for us
            page = Self::get_last_map_padding_page(self.state.get_new_value() + 1);
        }
        
        self.page_address.data = Address::from_page(page);
        self.page_address.update_address_record = true;
    }
    
    /// It must be ok to write to `page_address.record` and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// Prepares the next data address considering the size of the data about ot be written
    pub fn prepare_data_page(&mut self, data_size: u32, mut from_prepare: bool) -> Result<Address<C>, NvsError<K, T>>
    {
        // loop prepare page
        let mut pp = PreparePage::Repeat;
        while let PreparePage::Repeat = pp
        {
            pp = self.prepare_page_inner(data_size, from_prepare);
            from_prepare = false;
        }
        
        return match pp
        {
            PreparePage::NextAddress(address) => Ok(address),
            PreparePage::Fail(err) => Err(err),
            _ => panic!()
        };
    }
    fn prepare_page_inner(&mut self, data_size: u32, from_prepare: bool) -> PreparePage<K, T, C>
    {
        // loop around
        let page = self.page_address.get_record_page();
        if Self::is_page_overrun(page)
        {
            self.next_data_page();
            return PreparePage::Repeat;
        }
        
        // was only called to fix page wrap
        if data_size == 0
        {
            return PreparePage::NextAddress(self.page_address.data);
        }
        
        // new page - make checks
        if self.page_address.data.is_page_start()
        {
            if from_prepare
            {
                // bounds check in partition done on next prepare_data_page
                return PreparePage::NextAddress(self.page_address.data + data_size);
            }
            
            // brand new empty page
            if self.key_map.is_page_free(page)
            {
                if let Err(err) = self.erase_page(page)
                {
                    return PreparePage::Fail(err);
                }
                return PreparePage::NextAddress(self.page_address.data + data_size);
            }
            
            // on a new page
            self.page_address.update_address_record = true;
            let potential_space = self.key_map.get_available_page_space(page, &self.ignore);
            
            // let min_space = data_size * C::REWRITE_COPY_SIZE_MULTIPLIER as u32;
            // should not use this page - next page and prepare again
            if !C::should_rewrite_page(potential_space, data_size)
            {
                self.next_data_page();
                return PreparePage::Repeat;
            }
            
            // rewrite page - page_address.data is already at the start of the page
            // moves the page to itself
            if let Err(err) = self.move_data_page(page, true, true)
            {
                return PreparePage::Fail(err);
            }
            // move_data_page ended up moving more data than expected - next page and prepare again
            if !C::should_rewrite_page(self.page_address.data.get_remaining_space(), data_size)
            {
                self.next_data_page();
                return PreparePage::Repeat;
            }
            // bounds check in partition done on next prepare_data_page
            return PreparePage::NextAddress(self.page_address.data + data_size);
        }
        
        // there is enough space to continue writing to this page
        let space = self.page_address.data.get_remaining_space();
        if space <= data_size
        {
            // bounds check in partition done on next prepare_data_page
            return PreparePage::NextAddress(self.page_address.data + data_size);
        }
        
        // will probably be a new page
        self.page_address.update_address_record = true;
        
        // make sure next page is usable
        if self.page_in_map_padding(page + 1) || Self::is_page_overrun(page + 1)
        {
            self.next_data_page();
            return PreparePage::Repeat;
        }
        
        // make sure next page is ok to write to
        let overflow_size = data_size - space;
        let potential_space = self.key_map.get_available_page_space(page + 1, &self.ignore);
        
        // should not use the page - so no overflow allowed - next page and prepare again
        if !C::should_rewrite_page(potential_space, overflow_size)
        {
            self.next_data_page();
            return PreparePage::Repeat;
        }
        
        // leave page_address.data as it is, but the next address is going to be after the next pages rewritten data
        let nda = self.page_address.data;
        // skip over what were going to write
        self.page_address.data += data_size;
        
        let last_nra_page = self.page_address.get_record_page();
        // moves the page to itself
        if let Err(err) = self.move_data_page(page + 1, true, false)
        {
            return PreparePage::Fail(err);
        }
        let new_nra_page = self.page_address.get_record_page();
        
        // our current page has been overrun by the map
        if Self::page_in_range(page, last_nra_page, new_nra_page)
        {
            // check the new page_address.data
            return PreparePage::Repeat;
        }
        
        let next_nda = self.page_address.data;
        self.page_address.data = nda;
        // bounds check in partition done on next prepare_data_page
        return PreparePage::NextAddress(next_nda);
    }
    
    /// It must be ok to write to `page_address.record` and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// This function will also call `prepare_map` ready for the next record write
    pub fn move_data_page(&mut self, page: u32, erase: bool, from_prepare: bool) -> Result<(), NvsError<K, T>>
    {
        // make sure this instances page data is in cache
        self.load_page(page)?;
        if erase
        {
            // incase we need to prepare for new records or data
            self.erase_page(page)?;
        }
        // when reorganising a page to fit more on in prepare_data_page
        let page_rewrite = self.page_address.get_data_page() == page;
        
        let iter = match self.key_map.iter_page_values(page)
        {
            Some(i) => i,
            None => return Err(NvsError::MissingPageData)
        };
        // iterate through items that need moving
        for tr in iter
        {
            let key = tr.get_key();
            // skip ignore - true because we are clearing their data here
            if (self.ignore)(key, tr.key_map, true) { continue; }
            // consider overflow data
            
            let tv = tr.get_current_value();
            // ignore overflow entries whose data ends on this page (not starts)
            if tv.is_overflow_on(page)
            {
                // if we are writing to this page and page_address.data is at the start
                // then we want to retain the overflowed data - the other half is still in place
                if page_rewrite && from_prepare && self.page_address.data.is_page_start()
                {
                    // write the overflow from the previous page back to where it was
                    let size = tv.get_overflow_size() as u16;
                    // dont need to change ignore, from_prepare is true
                    let mut shadow_copy = NvsShadow::<'_, _, _, C, _, KEY_COUNT>::new(self.partition, tr.key_map,
                        self.page_address, self.cache, self.state, &self.ignore);
                    shadow_copy.write_entry_data(PageData::Cache(page, 0..size), PageData::None, true)?;
                }
                continue;
            }
            
            // gets data from the cached page - using either another page cache or reading from flash
            // to get any overflow data as well
            let (data, extra_data) = Self::get_tv_data(self.partition, self.cache, tv, page)?;
            
            let addr;
            // write data first - so that page checks are not disrupted by our updated record data
            if page_rewrite
            {
                let mut shadow_copy = NvsShadow::<'_, _, _, C, _, KEY_COUNT>::new(self.partition, tr.key_map, self.page_address,
                // only add this current entry to the ignore
                // - cannot check by page as previous iterations are now written to this page
                    self.cache, self.state, |k, km, clear| k == key || (self.ignore)(k, km, clear));
                addr = shadow_copy.write_entry_data(data, extra_data, from_prepare)?;
            }
            else
            {
                let mut shadow_copy = NvsShadow::<'_, _, _, C, _, KEY_COUNT>::new(self.partition, tr.key_map, self.page_address,
                // add the entries on this page to the ignore - only do this if we are writing to a new page
                // (because all records listed to this page are the ones about to be moved)
                    self.cache, self.state, |k, km, clear| (self.ignore)(k, km, clear) || km.is_key_on_page(k, page));
                addr = shadow_copy.write_entry_data(data, extra_data, from_prepare)?;
            }
            
            // actually write record
            let tv = tr.get_current_value();
            let rec_addr = NvsShadow::<_, _, C, F, KEY_COUNT>::write_record(self.partition, self.state, &mut self.page_address.record, tv, addr)?;
            let record = tv.to_record_new_addr(addr);
            // and update map
            if tr.key_map.update_record(record, rec_addr).is_none()
            {
                return Err(NvsError::MissingKey(key));
            }
            
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _, KEY_COUNT>::new(self.partition, tr.key_map, self.page_address,
                self.cache, self.state, &self.ignore);
            // call in preparation for the next entry to be moved
            shadow_copy.prepare_map()?;
        }
        
        return Ok(());
    }
    
    
    /// It must be ok to write to `page_address.record` and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// `data1` and `data2` both must be aligned to `WRITE_SIZE` individually
    pub fn write_entry_data(&mut self, data1: PageData, data2: PageData, from_prepare: bool) -> Result<Address<C>, NvsError<K, T>>
    {
        let (d1_len, d2_len) = (data1.len(), data2.len());
        let size = d1_len + d2_len;
        let next_addr = self.prepare_data_page(size as u32, from_prepare)?;
        
        let addr = self.page_address.data;
        // can safely write to page_address.data
        if d1_len > 0
        {
            let bytes = self.cache.get_data(&data1).ok_or(NvsError::MissingCacheData)?;
            map_err!{self.partition.write(addr.0, bytes)}?;
        }
        if d2_len > 0
        {
            let bytes = self.cache.get_data(&data2).ok_or(NvsError::MissingCacheData)?;
            map_err!{self.partition.write(addr.0 + d1_len as u32, bytes)}?;
        }
        
        // increment data address
        self.page_address.data = next_addr;
        return Ok(addr);
    }
    #[must_use]
    fn get_tv_data(partition: &mut T, cache: &mut PageCache, tv: &TableValue<K, C>,
        page: u32) -> Result<(PageData<'static>, PageData<'static>), NvsError<K, T>>
    {
        let overflow_size = tv.get_overflow_size();
        let data_footprint = tv.get_data_footprint();
        let offset = tv.get_address().get_page_offset();
        let data_end = offset + data_footprint - overflow_size;
        
        // read overflow if necessary
        let extra_data = match overflow_size
        {
            0 => PageData::None,
            _ => Self::get_overflow_data(partition, cache, page + 1, overflow_size as usize)?
        };
        
        return Ok((PageData::Cache(page, (offset as u16)..(data_end as u16)), extra_data));
        // return Ok((PageData::None, PageData::None));
    }
}