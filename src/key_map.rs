mod iter;
use hashbrown::HashMap;
pub use iter::*;

use core::cmp::Ordering;
use alloc::boxed::Box;
use enum_table::EnumTable;

use crate::{NvsKey, data::{Address, Record}, linked_list::LinkedList, round_up};

#[derive(Debug, Clone, Copy)]
pub struct TableValue<K: NvsKey, const PAGE_SIZE: u32>
{
    record_address: Address<PAGE_SIZE>,
    data_address: Address<PAGE_SIZE>,
    data_size: u16,
    key: K
}
impl<K: NvsKey, const PAGE_SIZE: u32> TableValue<K, PAGE_SIZE>
{
    #[inline]
    #[must_use]
    pub fn from_record(record: Record<PAGE_SIZE>, ra: Address<PAGE_SIZE>) -> Self
    {
        return Self { record_address: ra, data_address: record.address, data_size: record.size, key: record.get_key() };
    }
    
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
    pub fn set_record(&mut self, addr: Address<PAGE_SIZE>)
    {
        self.record_address = addr;
    }
    #[inline]
    #[must_use]
    pub fn to_record_new_addr(&self, new_addr: Address<PAGE_SIZE>) -> Record<PAGE_SIZE>
    {
        return Record { size: self.data_size, key: self.key.get_key_value(), address: new_addr };
    }
    #[inline]
    #[must_use]
    pub fn get_size(&self) -> u16
    {
        return self.data_size;
    }
    #[inline]
    pub fn set_size(&mut self, size: u16)
    {
        self.data_size = size;
    }
    #[inline]
    #[must_use]
    pub fn get_end_page(&self) -> u32
    {
        return (self.data_address + (self.data_size as u32 - 1)).get_page();
    }
    #[inline]
    #[must_use]
    pub fn is_on_page(&self, page: u32) -> bool
    {
        return self.data_address.get_page() == page || self.get_end_page() == page;
    }
    #[inline]
    #[must_use]
    pub fn is_overflow_on(&self, page: u32) -> bool
    {
        return self.data_address.get_page() != page && self.get_end_page() == page;
    }
    #[must_use]
    pub fn get_overflow_size(&self, ws: u32) -> u32
    {
        let next_addr = self.get_next_address(ws);
        let end_page = next_addr.get_page();
        // ok to do it this way - if next_addr is the start of the next page, we will still return 0
        if self.data_address.get_page() != end_page
        {
            return 0;
        }
        
        return next_addr.0 - Address::<PAGE_SIZE>::from_page(end_page).0;
    }
    #[inline]
    #[must_use]
    pub fn get_data_footprint(&self, ws: u32) -> u32
    {
        let size = self.data_size as u32;
        return round_up!(size, ws);
    }
}

pub struct KeyMap<K: NvsKey, const PAGE_SIZE: u32, const WS: usize>
{
    // index by key
    key_table: Box<EnumTable<K, u16, { K::LEN }>>,
    // static linked list ordered by page (address)
    linked_list: Box<LinkedList<TableValue<K, PAGE_SIZE>, { K::LEN }>>,
    // value is index into linked list
    page_table: HashMap<u32, u16>
}

impl<K: NvsKey, const PAGE_SIZE: u32, const WS: usize> KeyMap<K, PAGE_SIZE, WS>
{
    pub fn new() -> Self
    {
        return Self {
            key_table: Box::new(EnumTable::new_with_fn(|_| 0xFFFF)),
            linked_list: LinkedList::new(),
            page_table: HashMap::with_capacity(K::LEN)
        };
    }
    
    fn tv_cmp(l: &TableValue<K, PAGE_SIZE>, r: &TableValue<K, PAGE_SIZE>) -> Ordering
    {
        return l.data_address.cmp(&r.data_address);
    }
    
    #[must_use]
    /// doesnt update page data
    pub fn add_value(&mut self, record: Record<PAGE_SIZE>, ra: Address<PAGE_SIZE>) -> bool
    {
        return self.add_value_inner(record, ra).is_some();
    }
    #[must_use]
    fn add_value_inner(&mut self, record: Record<PAGE_SIZE>, ra: Address<PAGE_SIZE>) -> Option<u16>
    {
        let key = record.get_key();
        // cannot have duplicate keys
        if self.key_table.get(&key) != &0xFFFF
        {
            return None;
        }
        
        let value = TableValue::from_record(record, ra);
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
    pub fn update_record(&mut self, record: Record<PAGE_SIZE>, ra: Address<PAGE_SIZE>) -> Option<Address<PAGE_SIZE>>
    {
        let key = record.get_key();
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
        
        let value = TableValue::from_record(record, ra);
        
        // actually update value - changing order if necessary
        if !self.linked_list.update_value(Self::tv_cmp, index, value)
        {
            return None;
        }
        
        // remove page_table entries if necessary
        self.page_table_old(old_page, index, next_index);
        self.page_table_old(old_end_page, index, next_index);
        
        // add new page to table if this is the first value on that page
        self.page_table_new(record.address, record.size, index);
        
        return Some(old_record_addr);
    }
    fn page_table_old(&mut self, old_page: u32, index: u16, next_index: u16)
    {
        // was the first in the page
        let p_start = self.page_table.get_mut(&old_page).unwrap();
        if p_start == &index
        {
            // dont need to check end page - if the next item is on the same page, it will start on that page
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
    pub fn get_page_next_address(&self, page: u32) -> Option<Address<PAGE_SIZE>>
    {
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return Some(Address::from_page(page)),
        };
        
        let mut node = self.linked_list.get_node(index);
        // something is wrong if the first node is not actually on the page - should not occur
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
    #[inline]
    /// can include the previous page items if it has data on that page
    pub fn iter_page_values<'a>(&'a mut self, page: u32) -> Option<impl Iterator<Item = TableRecord<'a, K, PAGE_SIZE, WS>>>
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
    #[inline]
    /// Iterates through all records whose record address is on a page
    pub fn iter_map_page_values<'a>(&'a mut self, page: u32) -> impl Iterator<Item = TableRecord<'a, K, PAGE_SIZE, WS>>
    {
        return MapPageValueIter::new(self, page);
    }
    #[must_use]
    pub fn get_available_page_space(&self, page: u32) -> u32
    {
        let mut space = PAGE_SIZE;
        
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return space
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
    #[inline]
    #[must_use]
    pub fn is_page_free(&self, page: u32) -> bool
    {
        // includes overflow entries as well
        return !self.page_table.contains_key(&page);
    }
    #[inline]
    #[must_use]
    pub fn is_key_on_page(&self, key: K, page: u32) -> bool
    {
        return self.get_table_value(key).is_some_and(|tv| tv.is_on_page(page));
    }
    // #[inline]
    // #[must_use]
    // pub fn does_page_overflow(&self, page: u32) -> bool
    // {
    //     let index = match self.page_table.get(&(page + 1))
    //     {
    //         Some(i) => *i,
    //         None => return false
    //     };
        
    //     // is the first entry on the next page also on the queried page
    //     return self.linked_list.get_value(index).is_on_page(page);
    // }
    
    #[must_use]
    /// if new value is on a page with values already - its address will be greater
    pub fn add_value_page(&mut self, record: Record<PAGE_SIZE>, ra: Address<PAGE_SIZE>) -> bool
    {
        let index = match self.add_value_inner(record, ra)
        {
            Some(i) => i,
            None => return false
        };
        
        self.page_table_new(record.address, record.size, index);
        
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
        let end_page = (da + (size as u32 - 1)).get_page();
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
