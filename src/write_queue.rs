use hashbrown::HashMap;

use crate::NvsKey;

pub struct WriteQueue<'a, K: NvsKey>
{
    hash_map: HashMap<K, (&'a [u8], bool)>
}

impl<'a, K: NvsKey> WriteQueue<'a, K>
{
    #[inline]
    #[must_use]
    pub(crate) fn new<'b>(hash_map: HashMap<K, (&'b [u8], bool)>) -> Self
    {
        let hash_map = unsafe { core::mem::transmute(hash_map) };
        return Self { hash_map };
    }
    #[inline]
    #[must_use]
    pub(crate) fn get_back<'b>(self) -> HashMap<K, (&'b [u8], bool)>
    {
        return unsafe { core::mem::transmute(self.hash_map) };
    }
    
    #[inline]
    pub fn write_key_value<V: bytemuck::Pod>(&mut self, key: K, value: &'a V, force: bool)
    {
        self.hash_map.insert(key, (bytemuck::bytes_of(value), force));
    }
    #[inline]
    pub fn write_key_values<V: bytemuck::Pod>(&mut self, key: K, values: &'a [V], force: bool)
    {
        self.hash_map.insert(key, (bytemuck::cast_slice(values), force));
    }
}