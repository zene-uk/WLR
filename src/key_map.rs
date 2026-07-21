mod iter;
pub use iter::*;

use core::cmp::Ordering;

use alloc::boxed::Box;
use enum_table::EnumTable;
use micromap::Map;

use crate::{CheckConst, NvsKey, True, data::Address, linked_list::LinkedList, round_up};

#[derive(Debug, Clone, Copy)]
pub struct TableValue<K: NvsKey, const PAGE_SIZE: u32>
{
    record_address: Address<PAGE_SIZE>,
    data_address: Address<PAGE_SIZE>,
    data_size: u16,
    key: K
}
impl<K: NvsKey, const PAGE_SIZE: u32> TableValue<K, PAGE_SIZE>
    where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
{
    #[inline]
    #[must_use]
    pub fn get_next_address(&self, ws: u32) -> Address<PAGE_SIZE>
    {
        let end = self.data_address.0 + self.data_size as u32;
        // round up to write size
        return (round_up!(end, ws)).into();
    }
    #[inline]
    #[must_use]
    pub fn get_address(&self) -> Address<PAGE_SIZE>
    {
        return self.data_address;
    }
    #[inline]
    #[must_use]
    pub fn get_record(&self) -> Address<PAGE_SIZE>
    {
        return self.record_address;
    }
    #[inline]
    #[must_use]
    pub fn get_size(&self) -> u16
    {
        return self.data_size;
    }
    #[inline]
    #[must_use]
    pub fn get_end_page(&self) -> u32
    {
        return Address::<PAGE_SIZE>(self.data_address.0 + self.data_size as u32 - 1).get_page();
    }
    #[inline]
    #[must_use]
    pub fn is_on_page(&self, page: u32) -> bool
    {
        return self.data_address.get_page() == page || self.get_end_page() == page;
    }
}

pub struct KeyMap<K: NvsKey, const PAGE_SIZE: u32, const WS: usize>
    where [(); K::COUNT]: ,
        CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True,
        // CheckConst<{ K::COUNT < 0xFFFF }>: True
{
    // index by key
    key_table: Box<EnumTable<K, u16, { K::COUNT }>>,
    // static linked list ordered by page (address)
    linked_list: Box<LinkedList<TableValue<K, PAGE_SIZE>, { K::COUNT }>>,
    // value is index into linked list
    page_table: Box<Map<u32, u16, { K::COUNT }>>
}

impl<K: NvsKey, const PAGE_SIZE: u32, const WS: usize> KeyMap<K, PAGE_SIZE, WS>
    where [(); K::COUNT]: ,
        CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True,
        // CheckConst<{ K::COUNT < 0xFFFF }>: True
{
    pub fn new() -> Self
    {
        return Self {
            key_table: Box::new(EnumTable::new_with_fn(|_| 0xFFFF)),
            linked_list: LinkedList::new(),
            page_table: Box::new(Map::new())
        };
    }
    
    fn tv_cmp(l: &TableValue<K, PAGE_SIZE>, r: &TableValue<K, PAGE_SIZE>) -> Ordering
    {
        return l.data_address.cmp(&r.data_address);
    }
    
    /// doesnt update page data
    pub fn add_value(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> bool
    {
        return self.add_value_inner(key, ra, da, size).is_some();
    }
    fn add_value_inner(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> Option<u16>
    {
        // cannot have duplicate keys
        if self.key_table.get(&key) == &0xFFFF
        {
            return None;
        }
        
        let value = TableValue { record_address: ra, data_address: da, data_size: size, key };
        let index = match self.linked_list.insert_sorted(Self::tv_cmp, value)
        {
            Some(i) => i,
            None => return None
        };
        
        self.key_table.set(&key, index);
        return Some(index);
    }
    
    #[must_use]
    pub fn get_table_value(&self, key: K) -> Option<TableValue<K, PAGE_SIZE>>
    {
        let index = *self.key_table.get(&key);
        if index == 0xFFFF
        {
            return None;
        }
        
        return Some(*self.linked_list.get_node(index).as_ref());
    }
    /// if new address is on a page with values already - its value will be greater than the ones already there
    /// returns the old record address
    pub fn update_record(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> Option<Address<PAGE_SIZE>>
    {
        let index = *self.key_table.get(&key);
        if index == 0xFFFF
        {
            return None;
        }
        
        let node = self.linked_list.get_node(index);
        let tv = node.as_ref();
        let old_record_addr = tv.record_address;
        let old_page = tv.data_address.get_page();
        let old_end_page = tv.get_end_page();
        // index of old next node
        let next_index = node.into_next();
        
        let value = TableValue { record_address: ra, data_address: da, data_size: size, key };
        
        // actually update value - changing order if necessary
        if !self.linked_list.update_value(Self::tv_cmp, index, value)
        {
            return None;
        }
        
        // remove page_table entries if necessary
        self.page_table_old(old_page, index, next_index);
        self.page_table_old(old_end_page, index, next_index);
        
        // add new page to table if this is the first value on that page
        self.page_table_new(da, size, index);
        
        return Some(old_record_addr);
    }
    fn page_table_old(&mut self, old_page: u32, index: u16, next_index: u16)
    {
        // was the first in the page
        let p_start = self.page_table.get_mut(&old_page).unwrap();
        if p_start == &index
        {
            let next_page = self.linked_list.get_node(next_index).as_ref().data_address.get_page();
            if old_page == next_page
            {
                *p_start = next_index;
            }
            // was the last value on that page
            else
            {
                self.page_table.remove(&old_page);
            }
        }
    }
    #[must_use]
    /// returns page address aligned to write size
    pub fn get_next_page_address(&self, page: u32) -> Option<Address<PAGE_SIZE>>
    {
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return Some(Address::from_page(page)),
        };
        
        let mut node = self.linked_list.get_node(index);
        // somethings wrong if the first node is not actually on the page
        let mut next_address = Address(0);
        while node.as_ref().is_on_page(page)
        {
            next_address = node.as_ref().get_next_address(WS as u32);
            node = self.linked_list.get_node(node.into_next());
        }
        
        // no more space on page
        if next_address.get_page() != page
        {
            return None;
        }
        
        return Some(next_address);
    }
    #[must_use]
    /// can include the previous page items if it has data on that page
    pub fn get_page_values<'a>(&'a mut self, page: u32) -> Option<impl Iterator<Item = TableRecord<'a, K, PAGE_SIZE, WS>>>
    {
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return None
        };
        
        return Some(PageValueIter::new(self, index, page));
        // return Some(page_filter::<_, _, WS>(self.linked_list.iter_mut_from(index), page));
    }
    #[must_use]
    pub fn get_available_page_space(&self, page: u32) -> u32
    {
        let mut space = PAGE_SIZE;
        
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return space,
        };
        
        // remove all space taken up by entries
        // includes the overflow space as that would end up being on
        // this page if the contents are rewritten
        let mut node = self.linked_list.get_node(index);
        while node.as_ref().is_on_page(page)
        {
            let tv = node.as_ref();
            
            let size = match tv.data_address.get_page() != page
            {
                // if we dont start on the current page - then only count the data on this page
                // (this is an overflow entry)
                true => tv.get_next_address(WS as u32).get_page_offset(),
                false =>
                {
                    let size = node.as_ref().data_size;
                    // round up to write size
                    round_up!(size as u32, WS as u32)
                }
            };
            space = space.saturating_sub(size);
            
            node = self.linked_list.get_node(node.into_next());
        }
        
        return space;
    }
    #[must_use]
    pub fn is_page_free(&self, page: u32) -> bool
    {
        // includes overflow entries as well
        return !self.page_table.contains_key(&page);
    }
    
    /// if new value is on a page with values already - its address will be greater
    pub fn add_value_page(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> bool
    {
        let index = match self.add_value_inner(key, ra, da, size)
        {
            Some(i) => i,
            None => return false
        };
        
        self.page_table_new(da, size, index);
        
        return true;
    }
    fn page_table_new(&mut self, da: Address<PAGE_SIZE>, size: u16, index: u16)
    {
        // add page if it doesnt exist already
        let page = da.get_page();
        if !self.page_table.contains_key(&page)
        {
            self.page_table.insert(page, index);
        }
        
        // dont need to check that next page is valid
        // this function shouldnt be called if the data goes over the end of the partition
        
        // check if last byte is on new page
        let end_page = Address::<PAGE_SIZE>(da.0 + size as u32 - 1).get_page();
        // overflow page - add it to page_table
        if end_page != page && !self.page_table.contains_key(&end_page)
        {
            self.page_table.insert(end_page, index);
        }
    }
    
    pub fn initialise(&mut self)
    {
        // self.linked_list.sort(tv_cmp);
        let mut last_page = u32::MAX;
        for (i, n) in self.linked_list.iter_index()
        {
            // self.key_table.set(&n.key, i as u16);
            let page = n.data_address.get_page();
            // add index of first item in page
            if page != last_page
            {
                last_page = page;
                self.page_table.insert(page, i as u16);
            }
        }
    }
}
