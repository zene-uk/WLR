use core::ops::Range;

use alloc::boxed::Box;
use hashbrown::HashMap;

// #[type_const]
// pub const MAX_COLD_COUNT: usize = 5;
macro_rules! MAX_COLD_COUNT {
    () => {
        5
    };
}

pub enum PageData<'a>
{
    Cache(u32, Range<u16>),
    Owed(Box<[u8]>),
    Borrowed(&'a [u8]),
    None
}
impl<'a> PageData<'a>
{
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize
    {
        return match self
        {
            PageData::Cache(_, range) => range.len(),
            PageData::Owed(items) => items.len(),
            PageData::Borrowed(items) => items.len(),
            PageData::None => 0,
        };
    }
}

pub struct PageCache
{
    hash_map: HashMap<i32, (Box<[u8]>, u16)>,
    cold_count: usize
}

impl PageCache
{
    #[inline]
    #[must_use]
    pub fn new() -> Self
    {
        // very unlikely to ever need to have more than 4 pages in memory at a time
        return Self { hash_map: HashMap::with_capacity(4), cold_count: 0 }
    }
    
    #[must_use]
    pub fn get_data<'a>(&'a self, page_data: &'a PageData<'a>) -> Option<&'a [u8]>
    {
        match page_data
        {
            PageData::Cache(page, range) =>
            {
                let page = *page as i32;
                let data = self.hash_map.get(&page)?;
                // haven't loaded the data
                if data.1 < range.end || data.0.len() < range.end as usize { return None; }
                return Some(&data.0[(range.start as usize)..(range.end as usize)]);
            },
            PageData::Owed(items) => Some(items.as_ref()),
            PageData::Borrowed(items) => Some(items),
            PageData::None => Some(&[])
        }
    }
    #[inline]
    #[must_use]
    pub fn get_page<'a>(&'a mut self, page: u32) -> Option<(&'a mut [u8], &'a mut u16)>
    {
        let page = page as i32;
        return self.hash_map.get_mut(&page).map(|v| (v.0.as_mut(), &mut v.1));
    }
    pub fn cache_page(&mut self, page: u32, data: Box<[u8]>, filled: u16) -> bool
    {
        let page = page as i32;
        if self.hash_map.contains_key(&page)
        {
            return false;
        }
        
        self.hash_map.insert(page, (data, filled));
        return true;
    }
    // pub fn drop_page(&mut self, page: u32) -> bool
    // {
    //     let page = page as i32;
    //     let data = match self.hash_map.remove(&page)
    //     {
    //         Some(d) => d,
    //         None => return false
    //     };
        
    //     // don't retain allocation
    //     if self.cold_count >= MAX_COLD_COUNT
    //     {
    //         return true;
    //     }
        
    //     // didn't drop an already existing cold page
    //     if self.hash_map.insert(-page, data).is_none()
    //     {
    //         self.cold_count += 1;
    //     }
    //     return true;
    // }
    pub fn drop_all_pages(&mut self)
    {
        let mut colds: [Option<Box<[u8]>>; MAX_COLD_COUNT!()] = [const { None }; MAX_COLD_COUNT!()];
        let mut i = 0;
        // only take the first 5 - drop the rest
        for (_, v) in self.hash_map.drain().take(MAX_COLD_COUNT!())
        {
            colds[i] = Some(v.0);
            i += 1;
        }
        
        for (i, data) in colds.into_iter().enumerate()
        {
            if let Some(d) = data
            {
                let index = (i + 1) as i32;
                // always use -(self.cold_count + 1) for cold keys
                self.hash_map.insert(-index, (d, 0));
            }
        }
        self.cold_count = 0;
    }
    #[must_use]
    pub fn get_or_alloc(&mut self, size: usize) -> Box<[u8]>
    {
        if self.cold_count == 0
        {
            return unsafe { Box::new_uninit_slice(size).assume_init() };
        }
        
        let mut key = 0;
        for (&k, v) in self.hash_map.iter()
        {
            if k < 0 && v.0.len() >= size
            {
                key = k;
                break;
            }
        }
        
        self.cold_count -= 1;
        // key was just found - so it won't return none
        return self.hash_map.remove(&key).unwrap().0;
    }
    pub fn return_cold(&mut self, bytes: Box<[u8]>)
    {
        // don't retain allocation
        if self.cold_count >= MAX_COLD_COUNT!() { return; }
        
        self.cold_count += 1;
        self.hash_map.insert(-(self.cold_count as i32), (bytes, 0));
    }
}