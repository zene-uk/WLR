use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, data::Address, key_map::TableValue, nvs::NvsShadow};

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    pub fn next_data_page(&mut self)
    {
        let mut page = self.next_data_address.get_page() + 1;
        let last_padding_page = self.state.get_value() + C::MAP_POST_PADDING as u32;
        if !self.can_use_page(page)
        {
            page = last_padding_page + 1;
        }
        
        *self.next_data_address = Address::from_page(page);
    }
    #[must_use]
    #[inline]
    fn can_use_page(&self, page: u32) -> bool
    {
        let back_map_page = self.state.get_value();
        let last_padding_page = back_map_page + C::MAP_POST_PADDING as u32;
        return last_padding_page < page ||
            back_map_page - C::MAP_PRE_PADDING as u32 > page;
    }
    
    /// Prepares the next data address considering the size of the data about ot be written
    pub fn prepare_data_page(&mut self, data_size: u32) -> bool
    {
        // loop prepare page
        while {
            match self.prepare_page_inner(data_size)
            {
                Some(l) => l,
                None => return false
            }
        } { }
        
        return true;
    }
    fn prepare_page_inner(&mut self, data_size: u32) -> Option<bool>
    {
        let page = self.next_record_address.get_page();
        // brand new empty page
        if self.key_map.is_page_free(page)
        {
            if self.erase_page(page)
            {
                return Some(false);
            }
            return None;
        }
        // new page - make checks
        if self.next_record_address.is_page_start()
        {
            let potential_space = self.key_map.get_available_page_space(page);
            
            // should not use this page - next page and prepare again
            if potential_space < data_size * C::REWRITE_COPY_SIZE_MULTIPLIER as u32
            {
                self.next_data_page();
                return Some(true);
            }
            
            // TODO: recreate page
            return Some(false);
        }
        
        // there is enough space to continue writing to this page
        let space = self.next_record_address.get_remaining_space();
        if space <= data_size
        {
            return Some(false);
        }
        
        // make sure next page is usable
        if !self.can_use_page(page + 1)
        {
            self.next_data_page();
            return Some(true);
        }
        
        // make sure next page is ok to write to
        let overflow_size = data_size - space;
        let potential_space = self.key_map.get_available_page_space(page);
        
        // should not use the page - so no overflow allowed - next page and prepare again
        if potential_space < overflow_size * C::REWRITE_COPY_SIZE_MULTIPLIER as u32
        {
            self.next_data_page();
            return Some(true);
        }
        
        // TODO: recreate page
        return Some(false);
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
            if !shadow_copy.write_entry_data(data, &extra_data)
            {
                return false;
            }
            
            let tv = tr.get_current_value();
            if !NvsShadow::<_, _, C, F>::write_record(self.partition, self.next_record_address, tv, addr, unused_map_page)
            {
                return false;
            }
            // and update map
            let size = tv.get_size();
            if tr.key_map.update_record(tr.get_key(), rec_addr, addr, size).is_none()
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
    pub fn write_entry_data(&mut self, data1: &[u8], data2: &[u8]) -> bool
    {
        let size = data1.len() + data2.len();
        self.prepare_data_page(size as u32);
        
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