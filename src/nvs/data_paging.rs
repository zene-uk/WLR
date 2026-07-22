use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, data::Address, key_map::TableValue, nvs::NvsShadow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreparePage<const PAGE_SIZE: u32>
{
    NextAddress(Address<PAGE_SIZE>),
    Repeat,
    Fail
}

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    pub fn next_data_page(&mut self)
    {
        let mut page = self.next_data_address.get_page() + 1;
        // loop around
        if Self::is_page_overrun(page)
        {
            page = C::STATE_PAGES as u32;
        }
        if !self.page_in_map_padding(page)
        {
            // plus one inside so that it wraps around for us
            page = Self::get_last_map_padding_page(self.state.get_value() + 1);
        }
        
        *self.next_data_address = Address::from_page(page);
    }
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// Prepares the next data address considering the size of the data about ot be written
    pub fn prepare_data_page(&mut self, data_size: u32, unused_map_page: u32) -> Option<Address<{ C::PAGE_SIZE }>>
    {
        // loop prepare page
        let mut pp = PreparePage::Repeat;
        while pp == PreparePage::Repeat
        {
            pp = self.prepare_page_inner(data_size, unused_map_page);
        }
        
        return match pp
        {
            PreparePage::NextAddress(address) => Some(address),
            _ => None
        };
    }
    fn prepare_page_inner(&mut self, data_size: u32, unused_map_page: u32) -> PreparePage<{ C::PAGE_SIZE }>
    {
        // loop around
        let page = self.next_data_address.get_page();
        if Self::is_page_overrun(page)
        {
            self.next_data_page();
            return PreparePage::Repeat;
        }
        
        // was only called to fix page wrap
        if data_size == 0
        {
            return PreparePage::NextAddress(*self.next_data_address);
        }
        
        // brand new empty page
        if self.key_map.is_page_free(page)
        {
            if self.erase_page(page)
            {
                return PreparePage::NextAddress(*self.next_data_address + data_size);
            }
            return PreparePage::Fail;
        }
        // new page - make checks
        if self.next_data_address.is_page_start()
        {
            let potential_space = self.key_map.get_available_page_space(page);
            
            // should not use this page - next page and prepare again
            if potential_space < data_size * C::REWRITE_COPY_SIZE_MULTIPLIER as u32
            {
                self.next_data_page();
                return PreparePage::Repeat;
            }
            
            // rewrite page - next_data_address is already at the start of the page
            // moves the page to itself
            if !self.move_data_page(page, true, unused_map_page)
            {
                return PreparePage::Fail;
            }
            // bounds check in partition done on next prepare_data_page
            return PreparePage::NextAddress(*self.next_data_address + data_size);
        }
        
        // there is enough space to continue writing to this page
        let space = self.next_data_address.get_remaining_space();
        if space <= data_size
        {
            // bounds check in partition done on next prepare_data_page
            return PreparePage::NextAddress(*self.next_data_address + data_size);
        }
        
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
        let nda = *self.next_data_address;
        // skip over what were going to write
        *self.next_data_address += data_size;
        
        let last_nra_page = self.next_record_address.get_page();
        // moves the page to itself
        if !self.move_data_page(page + 1, true, unused_map_page)
        {
            return PreparePage::Fail;
        }
        let new_nra_page = self.next_record_address.get_page();
        
        // our current page has been overrun by the map
        if Self::page_in_range(page, last_nra_page, new_nra_page)
        {
            // check the new next_data_address
            return PreparePage::Repeat;
        }
        
        let next_nda = *self.next_data_address;
        *self.next_data_address = nda;
        // bounds check in partition done on next prepare_data_page
        return PreparePage::NextAddress(next_nda);
    }
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// This function will also call `prepare_map` ready for the next record write
    pub fn move_data_page(&mut self, page: u32, erase: bool, unused_map_page: u32) -> bool
    {
        // need to always reallocate due to potential recursion
        let page_data = match self.read_page(page)
        {
            Some(d) => d,
            None => return false
        };
        if erase
        {
            // incase we need to prepare for new records
            if !self.erase_page(page)
            {
                return false;
            }
        }
        
        let iter = match self.key_map.get_page_values(page)
        {
            Some(i) => i,
            None => return false
        };
        // iterate through items that need moving
        for tr in iter
        {
            // skip ignore
            if (self.ignore)(tr.get_key()) { continue; }
            // consider overflow data
            
            let tv = tr.get_current_value();
            let (data, extra_data) = match Self::get_tv_data(self.partition, tv, &page_data, page)
            {
                Some(v) => v,
                None => return false
            };
            let addr = *self.next_data_address;
            let rec_addr = *self.next_record_address;
            
            // write data first - so that page checks are not disrupted by our updated record data
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.next_data_address, self.next_record_address, self.state, &self.ignore);
            if !shadow_copy.write_entry_data(data, &extra_data, unused_map_page)
            {
                return false;
            }
            
            let tv = tr.get_current_value();
            let record = tv.to_record_new_addr(addr);
            if !NvsShadow::<_, _, C, F>::write_record(self.partition, self.next_record_address, tv, addr, unused_map_page)
            {
                return false;
            }
            // and update map
            if tr.key_map.update_record(record, rec_addr).is_none()
            {
                return false;
            }
            
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.next_data_address, self.next_record_address, self.state, &self.ignore);
            // call in preparation for the next entry to be moved
            if !shadow_copy.prepare_map()
            {
                return false;
            }
        }
        
        return true;
    }
    
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// `data1` and `data2` both must be aligned to `WRITE_SIZE` individually
    pub fn write_entry_data(&mut self, data1: &[u8], data2: &[u8], unused_map_page: u32) -> bool
    {
        let size = data1.len() + data2.len();
        self.prepare_data_page(size as u32, unused_map_page);
        
        // can safely write to next_data_address
        if self.partition.write(self.next_data_address.0, data1).is_err()
        {
            return false;
        }
        if self.partition.write(self.next_data_address.0 + data1.len() as u32, data2).is_err()
        {
            return false;
        }
        
        // increment data address
        *self.next_data_address = Address(self.next_data_address.0 + size as u32);
        return true;
    }
    #[must_use]
    fn get_tv_data<'b>(partition: &mut T, tv: &TableValue<K, { C::PAGE_SIZE }>, page_data: &'b [u8], page: u32) -> Option<(&'b [u8], Box<[u8]>)>
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
                if partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page + 1).0, &mut data).is_err()
                {
                    return None;
                }
                data
            }
        };
        
        return Some((&page_data[offset..data_end], extra_data));
    }
}