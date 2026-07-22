use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, data::Address, nvs::NvsShadow};

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    pub fn erase_page(&mut self, page: u32) -> bool
    {
        let offset = page * T::ERASE_SIZE as u32;
        return self.partition.erase(offset, offset + T::ERASE_SIZE as u32).is_ok();
    }
    pub fn read_page(&mut self, page: u32) -> Option<Box<[u8]>>
    {
        let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(T::ERASE_SIZE).assume_init() };
        
        if self.partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page as u32).0, &mut bytes).is_err()
        {
            return None;
        }
        
        return Some(bytes);
    }
    
    #[must_use]
    #[inline]
    pub fn is_page_overrun(page: u32) -> bool
    {
        return page >= C::TOTAL_PAGES;
    }
    #[must_use]
    pub fn page_in_map_padding(&self, page: u32) -> bool
    {
        let back_map_page = self.state.get_value();
        let last_padding_page = Self::get_last_map_padding_page(back_map_page);
        let first_padding_page = Self::get_first_map_padding_page(back_map_page);
        
        return Self::page_in_range(page, first_padding_page, last_padding_page);
    }
    #[must_use]
    pub fn page_in_range(page: u32, start: u32, end: u32) -> bool
    {
        // wraps around
        if start > end
        {
            return page >= start || page <= end;
        }
        // otherwise normal
        return start <= page &&
            end >= page;
    }
    #[must_use]
    pub fn get_last_map_padding_page(start_page: u32) -> u32
    {
        let page = start_page + C::MAP_POST_PADDING as u32;
        if page >= C::TOTAL_PAGES
        {
            return page - C::TOTAL_PAGES + C::STATE_PAGES as u32;
        }
        return page;
    }
    #[must_use]
    pub fn get_first_map_padding_page(start_page: u32) -> u32
    {
        let page = start_page as i32 - C::MAP_PRE_PADDING as i32;
        if page < C::STATE_PAGES as i32
        {
            return (page + C::TOTAL_PAGES as i32 - C::STATE_PAGES as i32) as u32;
        }
        return page as u32;
    }
}