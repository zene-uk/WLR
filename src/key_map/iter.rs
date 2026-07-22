use crate::{NvsKey, key_map::{KeyMap, TableValue}};

pub(super) struct PageValueIter<'a, K: NvsKey, const PAGE_SIZE: u32, const WS: usize>
{
    key_map: &'a mut KeyMap<K, PAGE_SIZE, WS>,
    /// Stores the index of the next node to be read.
    /// Means that we can safely update the value returned by `next`
    /// and it won't disrupt the iterator.
    current: u16,
    start: u16,
    page: u32
}

impl<'a, K: NvsKey, const PAGE_SIZE: u32, const WS: usize> PageValueIter<'a, K, PAGE_SIZE, WS>
{
    pub fn new(key_map: &'a mut KeyMap<K, PAGE_SIZE, WS>, start: u16, page: u32) -> Self
    {
        return Self { key_map, current: start, start, page };
    }
}
impl<'a, K: NvsKey, const PAGE_SIZE: u32, const WS: usize> Iterator for PageValueIter<'a, K, PAGE_SIZE, WS>
{
    type Item = TableRecord<'a, K, PAGE_SIZE, WS>;

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
            let ptr = self.key_map as *mut KeyMap<K, PAGE_SIZE, WS>;
            ptr.as_mut::<'a>().unwrap()
        };
        return Some(TableRecord { key_map, key, index });
    }
}

pub struct TableRecord<'a, K: NvsKey, const PAGE_SIZE: u32, const WS: usize>
{
    pub key_map: &'a mut KeyMap<K, PAGE_SIZE, WS>,
    key: K,
    index: u16
}
impl<'a, K: NvsKey, const PAGE_SIZE: u32, const WS: usize> TableRecord<'a, K, PAGE_SIZE, WS>
{
    #[inline]
    #[must_use]
    pub fn get_current_value(&'a self) -> &'a TableValue<K, PAGE_SIZE>
    {
        return self.key_map.linked_list.get_value(self.index);
    }
    #[inline]
    #[must_use]
    pub fn get_key(&self) -> K
    {
        return self.key;
    }
}