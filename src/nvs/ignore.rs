use crate::{NvsConstants, NvsKey, key_map::KeyMap};

pub(crate) trait Ignore<K: NvsKey, C: NvsConstants, const KEY_COUNT: usize>
{
    fn has_key(&self, key: K, key_map: &KeyMap<K, C, KEY_COUNT>, clear: bool) -> bool;
}


impl<K: NvsKey, C: NvsConstants, const KEY_COUNT: usize, T: Fn(K, &KeyMap<K, C, KEY_COUNT>, bool) -> bool> Ignore<K, C, KEY_COUNT> for T
{
    fn has_key(&self, key: K, key_map: &KeyMap<K, C, KEY_COUNT>, clear: bool) -> bool
    {
        return self(key, key_map, clear);
    }
}

pub(crate) struct IgnoreKey<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize>
{
    inner: &'a dyn Ignore<K, C, KEY_COUNT>,
    extra: K
}
impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> IgnoreKey<'a, K, C, KEY_COUNT>
{
    pub fn new(inner: &'a dyn Ignore<K, C, KEY_COUNT>, extra: K) -> Self
    {
        return Self { inner, extra };
    }
}
impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> Ignore<K, C, KEY_COUNT> for IgnoreKey<'a, K, C, KEY_COUNT>
{
    fn has_key(&self, key: K, key_map: &KeyMap<K, C, KEY_COUNT>, clear: bool) -> bool
    {
        return key == self.extra || self.inner.has_key(key, key_map, clear);
    }
}
pub(crate) struct IgnorePage<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize>
{
    inner: &'a dyn Ignore<K, C, KEY_COUNT>,
    page: u32
}
impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> IgnorePage<'a, K, C, KEY_COUNT>
{
    pub fn new(inner: &'a dyn Ignore<K, C, KEY_COUNT>, page: u32) -> Self
    {
        return Self { inner, page };
    }
}
impl<'a, K: NvsKey, C: NvsConstants, const KEY_COUNT: usize> Ignore<K, C, KEY_COUNT> for IgnorePage<'a, K, C, KEY_COUNT>
{
    fn has_key(&self, key: K, key_map: &KeyMap<K, C, KEY_COUNT>, clear: bool) -> bool
    {
        return self.inner.has_key(key, key_map, clear) || key_map.is_key_on_page(key, self.page);
    }
}