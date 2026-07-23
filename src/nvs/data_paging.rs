use core::panic;
use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{Ignore, NvsConstants, NvsError, NvsKey, data::Address, key_map::TableValue, map_err, nvs::NvsShadow};

#[derive(Debug)]
enum PreparePage<K: NvsKey, T: NorFlash, const PAGE_SIZE: u32>
{
    NextAddress(Address<PAGE_SIZE>),
    Repeat,
    Fail(NvsError<K, T>)
}

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Ignore<K, C>> NvsShadow<'a, K, T, C, F>
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
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// Prepares the next data address considering the size of the data about ot be written
    pub fn prepare_data_page(&mut self, data_size: u32, mut from_prepare: bool) -> Result<Address<{ C::PAGE_SIZE }>, NvsError<K, T>>
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
    fn prepare_page_inner(&mut self, data_size: u32, from_prepare: bool) -> PreparePage<K, T, { C::PAGE_SIZE }>
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
            let potential_space = self.key_map.get_available_page_space(page);
            
            let min_space = data_size * C::REWRITE_COPY_SIZE_MULTIPLIER as u32;
            // should not use this page - next page and prepare again
            if potential_space < min_space
            {
                self.next_data_page();
                return PreparePage::Repeat;
            }
            
            // TODO: case where this page contains some overflow data
            
            // rewrite page - next_data_address is already at the start of the page
            // moves the page to itself
            if let Err(err) = self.move_data_page(page, true, true)
            {
                return PreparePage::Fail(err);
            }
            // move_data_page ended up moving more data than expected - next page and prepare again
            if self.page_address.data.get_remaining_space() < min_space
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
        let potential_space = self.key_map.get_available_page_space(page + 1);
        
        // should not use the page - so no overflow allowed - next page and prepare again
        if potential_space < overflow_size * C::REWRITE_COPY_SIZE_MULTIPLIER as u32
        {
            self.next_data_page();
            return PreparePage::Repeat;
        }
        
        // leave next_data_address as it is, but the next address is going to be after the next pages rewritten data
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
            // check the new next_data_address
            return PreparePage::Repeat;
        }
        
        let next_nda = self.page_address.data;
        self.page_address.data = nda;
        // bounds check in partition done on next prepare_data_page
        return PreparePage::NextAddress(next_nda);
    }
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// This function will also call `prepare_map` ready for the next record write
    pub fn move_data_page(&mut self, page: u32, erase: bool, from_prepare: bool) -> Result<(), NvsError<K, T>>
    {
        // need to always reallocate due to potential recursion
        let page_data = self.read_page(page)?;
        if erase
        {
            // incase we need to prepare for new records
            self.erase_page(page)?;
        }
        
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
            // skip ignore
            if (self.ignore)(key, tr.key_map) { continue; }
            // consider overflow data
            
            // TODO: fix issues where overflow data could get here twice in the same prepare_map function
            // (if the first value is an overflow and we read data wrong - which could occur but very rare)
            // and also when we get to the last value to read its overflow data but thats now been overridden by records
            
            let tv = tr.get_current_value();
            let (data, extra_data) = Self::get_tv_data(self.partition, tv, &page_data, page)?;
            
            let addr;
            // write data first - so that page checks are not disrupted by our updated record data
            if page_rewrite
            {
                let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.page_address,
                // only add this current entry to the ignore
                    self.state, |k, km| k == key || (self.ignore)(k, km));
                addr = shadow_copy.write_entry_data(data, &extra_data, from_prepare)?;
            }
            else
            {
                let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.page_address,
                // add the entries on this page to the ignore - only do this if we are writing to a new page
                // (because the entries on this page are a mix of ones that exist and ones that dont)
                    self.state, |k, km| (self.ignore)(k, km) || km.is_key_on_page(k, page));
                addr = shadow_copy.write_entry_data(data, &extra_data, from_prepare)?;
            }
            
            let tv = tr.get_current_value();
            let rec_addr = NvsShadow::<_, _, C, F>::write_record(self.partition, self.state, &mut self.page_address.record, tv, addr)?;
            let record = tv.to_record_new_addr(addr);
            // and update map
            if tr.key_map.update_record(record, rec_addr).is_none()
            {
                return Err(NvsError::MissingKey(key));
            }
            
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.page_address, self.state, &self.ignore);
            // call in preparation for the next entry to be moved
            shadow_copy.prepare_map()?;
        }
        
        return Ok(());
    }
    
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// `data1` and `data2` both must be aligned to `WRITE_SIZE` individually
    pub fn write_entry_data(&mut self, data1: &[u8], data2: &[u8], from_prepare: bool) -> Result<Address<{ C::PAGE_SIZE }>, NvsError<K, T>>
    {
        let size = data1.len() + data2.len();
        let next_addr = self.prepare_data_page(size as u32, from_prepare)?;
        
        let addr = self.page_address.data;
        // can safely write to next_data_address
        map_err!{self.partition.write(addr.0, data1)}?;
        map_err!{self.partition.write(addr.0 + data1.len() as u32, data2)}?;
        
        // increment data address
        self.page_address.data = next_addr;
        return Ok(addr);
    }
    #[must_use]
    fn get_tv_data<'b>(partition: &mut T, tv: &TableValue<K, { C::PAGE_SIZE }>, page_data: &'b [u8], page: u32) -> Result<(&'b [u8], Box<[u8]>), NvsError<K, T>>
    {
        let overflow_size = tv.get_overflow_size(C::WRITE_SIZE as u32) as usize;
        let data_footprint = tv.get_data_footprint(C::WRITE_SIZE as u32) as usize;
        let offset = tv.get_address().get_page_offset() as usize;
        let data_end = offset + data_footprint - overflow_size;
        
        // read overflow if necessary
        let extra_data = match overflow_size
        {
            0 => unsafe { Box::<[u8]>::new_uninit_slice(0).assume_init() },
            _ =>
            {
                let mut data = unsafe { Box::<[u8]>::new_uninit_slice(overflow_size).assume_init() };
                map_err!{partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page + 1).0, &mut data)}?;
                data
            }
        };
        
        return Ok((&page_data[offset..data_end], extra_data));
    }
}