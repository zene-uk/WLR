use embedded_storage::nor_flash::NorFlash;

use crate::{Ignore, NvsConstants, NvsError, NvsKey, cache::{PageCache, PageData}, data::Address, map_err, nvs::NvsShadow, state::State};

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Ignore<K, C>> NvsShadow<'a, K, T, C, F>
{
    pub fn erase_page(&mut self, page: u32) -> Result<(), NvsError<K, T>>
    {
        let offset = page * C::PAGE_SIZE;
        return map_err!{self.partition.erase(offset, offset + C::PAGE_SIZE)};
    }
    #[must_use]
    pub fn load_page(&mut self, page: u32) -> Result<(), NvsError<K, T>>
    {
        if let Some((bytes, range_ref)) = self.cache.get_page(page)
        {
            let range = *range_ref;
            // page already loaded
            if range as u32 == C::PAGE_SIZE
            {
                return Ok(());
            }
            
            // need to load the rest of the page
            map_err!{self.partition.read(
                Address::<{ C::PAGE_SIZE }>::from_page_offset(page, range as u32).0,
                &mut bytes[(range as usize)..(C::PAGE_SIZE as usize)])}?;
            *range_ref = C::PAGE_SIZE as u16;
            
            return Ok(());
        }
        
        // nothing loaded - so need to read it all
        let mut bytes = self.cache.get_or_alloc(C::PAGE_SIZE as usize);
        map_err!{self.partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page as u32).0, &mut bytes)}?;
        self.cache.cache_page(page, bytes, C::PAGE_SIZE as u16);
        
        return Ok(());
    }
    #[must_use]
    pub fn get_overflow_data<'b>(partition: &mut T, cache: &mut PageCache, page: u32, size: usize) -> Result<PageData<'b>, NvsError<K, T>>
    {
        // just refernece page
        if let Some((bytes, range_ref)) =  cache.get_page(page)
        {
            let range = *range_ref;
            if range as usize >= size
            {
                return Ok(PageData::Cache(page, 0..(size as u16)));
            }
            
            // need to load more of the data
            map_err!{partition.read(
                Address::<{ C::PAGE_SIZE }>::from_page_offset(page, range as u32).0,
                &mut bytes[(range as usize)..size])}?;
            *range_ref = size as u16;
            
            return Ok(PageData::Cache(page, 0..(size as u16)));
        }
        
        // nothing loaded
        let mut data = cache.get_or_alloc(size);
        map_err!{partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page).0, &mut data[..size])}?;
        
        // we have the capacity for the entire page - add it cache
        if data.len() == C::PAGE_SIZE as usize
        {
            cache.cache_page(page, data, size as u16);
            return Ok(PageData::Cache(page, 0..(size as u16)));
        }
        
        // only allocated for the data we needed, so return it as owed
        return Ok(PageData::Owed(data));
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
        // the new and old or incase we are in the middle of moving map pages
        let last_padding_page = Self::get_last_map_padding_page(self.state.get_new_value());
        let first_padding_page = Self::get_first_map_padding_page(self.state.get_old_value());
        
        return Self::page_in_range(page, first_padding_page, last_padding_page);
    }
    #[must_use]
    /// inclusive range check
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
    
    #[inline]
    #[must_use]
    pub fn is_in_old_map_page(state: &State<T, C>, page: u32) -> bool
    {
        let new_first_page = state.get_new_value();
        let old_first_page = state.get_old_value();
        
        if new_first_page == old_first_page
        {
            return false;
        }
        
        return Self::page_in_range(page, old_first_page, new_first_page - 1);
    }
}