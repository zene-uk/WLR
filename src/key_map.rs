use core::cmp::Ordering;

use alloc::boxed::Box;
use enum_table::EnumTable;
use micromap::Map;

use crate::{CheckConst, NvsKey, True, data::Address, linked_list::LinkedList};

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
    pub fn get_next_address(&self) -> Address<PAGE_SIZE>
    {
        return (self.data_address.0 + self.data_size as u32).into();
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
}

pub struct KeyMap<K: NvsKey, const PAGE_SIZE: u32>
    where [(); K::COUNT]: ,
        CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True
{
    // index by key
    key_table: Box<EnumTable<K, u16, { K::COUNT }>>,
    // static linked list ordered by page (address)
    linked_list: Box<LinkedList<TableValue<K, PAGE_SIZE>, { K::COUNT }>>,
    // value is index into linked list
    page_table: Box<Map<u32, u16, { K::COUNT }>>
}

impl<K: NvsKey, const PAGE_SIZE: u32> KeyMap<K, PAGE_SIZE>
    where [(); K::COUNT]: ,
        CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True
{
    pub fn new() -> Self
    {
        return Self {
            key_table: Box::new(EnumTable::new_with_fn(|_| 0xFFFF)),
            linked_list: LinkedList::new(),
            page_table: Box::new(Map::new())
        };
    }
    
    /// used to init map
    pub fn insert_existing_value(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> bool
    {
        let value = TableValue { record_address: ra, data_address: da, data_size: size, key };
        return self.linked_list.add_value(value).is_some();
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
    pub fn update_record(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> bool
    {
        let index = *self.key_table.get(&key);
        if index == 0xFFFF
        {
            return false;
        }
        
        let node = self.linked_list.get_node(index);
        let tv = node.as_ref();
        let old_page = tv.data_address.get_page();
        // index of old next node
        let next_index = node.into_next();
        
        let value = TableValue { record_address: ra, data_address: da, data_size: size, key };
        
        // actually update value - changing order if necessary
        if !self.linked_list.update_value(tv_cmp, index, value)
        {
            return false;
        }
        
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
        
        // add new page to table if this is the first value on that page
        let new_page = da.get_page();
        if !self.page_table.contains_key(&new_page)
        {
            self.page_table.insert(new_page, index);
        }
        
        return true;
    }
    #[must_use]
    pub fn get_next_page_address(&self, page: u32) -> Option<Address<PAGE_SIZE>>
    {
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return Some(Address::from_page(page)),
        };
        
        let mut node = self.linked_list.get_node(index);
        let mut next_address = node.as_ref().get_next_address();
        while node.as_ref().data_address.get_page() == page
        {
            node = self.linked_list.get_node(node.into_next());
            next_address = node.as_ref().get_next_address();
        }
        
        // no more space on page
        if next_address.get_page() != page
        {
            return None;
        }
        
        return Some(next_address);
    }
    #[must_use]
    pub fn get_page_values(&mut self, page: u32) -> Option<impl Iterator<Item = &mut TableValue<K, PAGE_SIZE>>>
    {
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return None,
        };
        
        return Some(self.linked_list.iter_mut_from(index).take_while(move |v| v.data_address.get_page() == page));
    }
    
    /// if new value is on a page with values already - its address will be greater
    pub fn add_new_value(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> bool
    {
        // cannot have duplicate keys
        if self.key_table.get(&key) == &0xFFFF
        {
            return false;
        }
        
        let value = TableValue { record_address: ra, data_address: da, data_size: size, key };
        let index = match self.linked_list.insert_sorted(tv_cmp, value)
        {
            Some(i) => i,
            None => return false,
        };
        
        self.key_table.set(&key, index);
        
        // add page if it doesnt exist already
        let page = da.get_page();
        if !self.page_table.contains_key(&page)
        {
            self.page_table.insert(page, index);
        }
        
        return true;
    }
    
    pub fn initialise(&mut self)
    {
        self.linked_list.sort(tv_cmp);
        let mut last_page = u32::MAX;
        for (i, n) in self.linked_list.iter_any()
        {
            self.key_table.set(&n.key, i as u16);
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

fn tv_cmp<K: NvsKey, const PAGE_SIZE: u32>(l: &TableValue<K, PAGE_SIZE>, r: &TableValue<K, PAGE_SIZE>) -> Ordering
{
    return l.data_address.cmp(&r.data_address);
}
