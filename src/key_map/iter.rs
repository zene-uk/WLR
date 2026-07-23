use crate::{NvsConstants, NvsKey, key_map::{KeyMap, TableValue}};

pub struct TableRecord<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize>
{
    pub key_map: &'a mut KeyMap<K, C, KEY_COUNT>,
    key: K,
    index: u16
}
impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> TableRecord<'a, K, C, KEY_COUNT>
{
    #[inline]
    #[must_use]
    pub fn get_current_value(&self) -> &TableValue<K, C>
    {
        return self.key_map.linked_list.get_value(self.index);
    }
    #[inline]
    #[must_use]
    pub fn get_current_value_mut(&mut self) -> &mut TableValue<K, C>
    {
        return self.key_map.linked_list.get_value_mut(self.index);
    }
    #[inline]
    #[must_use]
    pub fn get_key(&self) -> K
    {
        return self.key;
    }
}

pub(super) struct PageValueIter<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize>
{
    key_map: &'a mut KeyMap<K, C, KEY_COUNT>,
    /// Stores the index of the next node to be read.
    /// Means that we can safely update the value returned by `next`
    /// and it won't disrupt the iterator.
    current: u16,
    start: u16,
    page: u32
}

impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> PageValueIter<'a, K, C, KEY_COUNT>
{
    pub fn new(key_map: &'a mut KeyMap<K, C, KEY_COUNT>, start: u16, page: u32) -> Self
    {
        return Self { key_map, current: start, start, page };
    }
}
impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> Iterator for PageValueIter<'a, K, C, KEY_COUNT>
{
    type Item = TableRecord<'a, K, C, KEY_COUNT>;

    fn next(&mut self) -> Option<Self::Item>
    {
        let mut current = self.current;
        if current == 0xFFFF
        {
            return None;
        }
        
        let node = self.key_map.linked_list.get_node(current);
        let tv = node.as_ref();
        
        // check it is on correct page
        if !tv.is_on_page(self.page)
        {
            self.current = current;
            return None;
        }
        
        let key = tv.key;
        let index = current;
        
        current = node.into_next();
        if current == self.start
        {
            current = 0xFFFF;
        }
        self.current = current;
        
        // force 'a lifetime
        // is ok as the original data has lifetime 'a and mut here will not be used twice
        let key_map = unsafe 
        {
            let ptr = self.key_map as *mut KeyMap<K, C, KEY_COUNT>;
            ptr.as_mut::<'a>().unwrap()
        };
        return Some(TableRecord { key_map, key, index });
    }
}

pub(super) struct MapPageValueIter<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize>
{
    key_map: &'a mut KeyMap<K, C, KEY_COUNT>,
    index: u16,
    page: u32
}

impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> MapPageValueIter<'a, K, C, KEY_COUNT>
{
    pub fn new(key_map: &'a mut KeyMap<K, C, KEY_COUNT>, page: u32) -> Self
    {
        return Self { key_map, index: 0, page };
    }
    
    fn next_inner(&mut self) -> Option<&TableValue<K, C>>
    {
        let index = self.index;
        if self.key_map.linked_list.len() <= self.index as usize
        {
            return None;
        }
        
        let tv = self.key_map.linked_list.get_value(self.index);
        self.index = index + 1;
        return Some(tv);
    }
}
impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> Iterator for MapPageValueIter<'a, K, C, KEY_COUNT>
{
    type Item = TableRecord<'a, K, C, KEY_COUNT>;

    fn next(&mut self) -> Option<Self::Item>
    {
        let page = self.page;
        let mut tv = self.next_inner()?;
        // skip all values on the wrong page
        while tv.record_address.get_page() != page
        {
            tv = self.next_inner()?;
        }
        
        let key = tv.key;
        // next_inner will set index to +1 of the current value
        let index = self.index - 1;
        
        // force 'a lifetime
        // is ok as the original data has lifetime 'a and mut here will not be used twice
        let key_map = unsafe 
        {
            let ptr = self.key_map as *mut KeyMap<K, C, KEY_COUNT>;
            ptr.as_mut::<'a>().unwrap()
        };
        return Some(TableRecord { key_map, key, index });
    }
}
