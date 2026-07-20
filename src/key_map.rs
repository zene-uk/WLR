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
}

pub struct KeyMap<K: NvsKey, const PAGE_SIZE: u32, const WS: usize>
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

impl<K: NvsKey, const PAGE_SIZE: u32, const WS: usize> KeyMap<K, PAGE_SIZE, WS>
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
        let index = match self.linked_list.insert_sorted(tv_cmp, value)
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
    /// returns page address algined to write size
    pub fn get_next_page_address(&self, page: u32) -> Option<Address<PAGE_SIZE>>
    {
        let addr = self.get_next_page_address_inner(page);
        
        // no more space on page
        if addr.get_page() != page
        {
            return None;
        }
        
        // there could be data from the previous page wrapping over
        // (if page_offset == 0, then we couldnt find any entries on this page)
        if addr.get_page_offset() == 0 && page != 0
        {
            let pre_addr = self.get_next_page_address_inner(page - 1);
            
            // this could just be to first byte on this page
            // if the last page is full but doesnt overflow
            if pre_addr.get_page() == page
            {
                return Some(pre_addr);
            }
        }
        
        return Some(addr);
    }
    fn get_last_index_on_page<'a>(&'a self, page: u32) -> Option<u16>
    {
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return None,
        };
        
        let mut node = self.linked_list.get_node(index);
        while node.as_ref().data_address.get_page() == page
        {
            node = self.linked_list.get_node(node.into_next());
        }
        // exits on first not on page - so previous is last on page
        return Some(node.into_previous());
        // return Some(self.linked_list.get_value(node.into_previous()));
    }
    fn get_next_page_address_inner(&self, page: u32) -> Address<PAGE_SIZE>
    {
        return match self.get_last_index_on_page(page)
        {
            Some(i) => self.linked_list.get_value(i).get_next_address(WS as u32),
            None => Address::from_page(page)
        };
    }
    #[must_use]
    /// can include the previous item if it has data on that page
    pub fn get_page_values(&mut self, page: u32) -> Option<impl Iterator<Item = &mut TableValue<K, PAGE_SIZE>>>
    {
        if page != 0
        {
            match self.get_last_index_on_page(page - 1)
            {
                Some(i) =>
                {
                    return Some(page_filter::<_, _, WS>(self.linked_list.iter_mut_from(i), page));
                },
                None => {}
            }
        }
        
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return None
        };
        
        return Some(page_filter::<_, _, WS>(self.linked_list.iter_mut_from(index), page));
    }
    #[must_use]
    pub fn get_remaining_page_space(&self, page: u32) -> u32
    {
        let mut space = PAGE_SIZE;
        
        // remove overflow from last page if there is any
        if page != 0
        {
            if let Some(i) = self.get_last_index_on_page(page - 1)
            {
                let tvna = self.linked_list.get_value(i).get_next_address(WS as u32);
                if tvna.get_page() == page
                {
                    space -= tvna.get_page_offset();
                }
            }
        }
        
        let index = match self.page_table.get(&page)
        {
            Some(i) => *i,
            None => return space,
        };
        
        // remove all space taken up by entries
        // includes the overflow space as that would end up being on
        // this page if the contents are rewritten
        let mut node = self.linked_list.get_node(index);
        while node.as_ref().data_address.get_page() == page
        {
            let size = node.as_ref().get_size();
            // round up to write size
            let size = round_up!(size as u32, WS as u32);
            space = space.saturating_sub(size);
            
            node = self.linked_list.get_node(node.into_next());
        }
        
        return space;
    }
    
    /// if new value is on a page with values already - its address will be greater
    pub fn add_value_page(&mut self, key: K, ra: Address<PAGE_SIZE>, da: Address<PAGE_SIZE>, size: u16) -> bool
    {
        let index = match self.add_value_inner(key, ra, da, size)
        {
            Some(i) => i,
            None => return false
        };
        
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

/// only allows items who start of end on a page
fn page_filter<'a, K: NvsKey, const PAGE_SIZE: u32, const WS: usize>(iter: impl Iterator<Item = &'a mut TableValue<K, PAGE_SIZE>>, page: u32)
    -> impl Iterator<Item = &'a mut TableValue<K, PAGE_SIZE>>
    where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
{
    return iter.take_while(move |v|
    {
        if v.data_address.get_page() == page
        {
            return true;
        }
        // must actually have data on the page
        let na = v.get_next_address(WS as u32);
        return na.get_page_offset() != 0 && na.get_page() == page;
    });
}

fn tv_cmp<K: NvsKey, const PAGE_SIZE: u32>(l: &TableValue<K, PAGE_SIZE>, r: &TableValue<K, PAGE_SIZE>) -> Ordering
{
    return l.data_address.cmp(&r.data_address);
}
