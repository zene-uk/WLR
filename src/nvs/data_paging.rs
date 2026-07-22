use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, data::Address, key_map::TableValue, nvs::NvsShadow};

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    pub fn next_data_page(&mut self)
    {
        let mut page = self.next_data_address.get_page() + 1;
        let back_map_page = self.state.get_value();
        let last_padding_page = back_map_page + C::MAP_POST_PADDING as u32;
        if last_padding_page >= page &&
            back_map_page - C::MAP_PRE_PADDING as u32 <= page
        {
            page = last_padding_page + 1;
        }
        
        *self.next_data_address = Address::from_page(page);
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
            self.erase_page(page);
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
            
            // write record first
            if !NvsShadow::<_, _, C, F>::write_record(self.partition, self.next_record_address, tv, addr, unused_map_page)
            {
                return false;
            }
            // and update map
            let size = tv.get_size();
            tr.key_map.update_record(tr.get_key(), rec_addr, addr, size);
            
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.next_data_address, self.next_record_address, self.state, &self.ignore);
            shadow_copy.write_entry_data(data, &extra_data);
            // call in preparation for the next entry to be moved
            shadow_copy.prepare_map();
        }
        
        return true;
    }
    
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// `data1` and `data2` both must be aligned to `WRITE_SIZE` individually
    pub fn write_entry_data(&mut self, data1: &[u8], data2: &[u8])
    {
        let size = data1.len() + data2.len();
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