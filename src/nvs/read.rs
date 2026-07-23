use core::{mem::MaybeUninit, slice};
use embedded_storage::nor_flash::NorFlash;

use crate::{Nvs, NvsConstants, NvsError, NvsKey, Padding, map_err, round_up};

impl<K: NvsKey, T: NorFlash, C: NvsConstants + 'static> Nvs<K, T, C>
{
    #[must_use]
    pub fn read_key_value_direct<V: bytemuck::Pod>(&mut self, key: K) -> Result<V, NvsError<K, T>>
    {
        let mut result: V = unsafe { MaybeUninit::zeroed().assume_init() };
        return self.read_key_value(key, &mut result).map(|_| result);
    }
    #[inline]
    pub fn read_key_value<V: bytemuck::Pod>(&mut self, key: K, out: &mut V) -> Result<(), NvsError<K, T>>
    {
        return self.read_key_values_inner(key, slice::from_mut(out), true);
    }
    #[inline]
    pub fn read_key_values<V: bytemuck::Pod>(&mut self, key: K, out: &mut [V]) -> Result<(), NvsError<K, T>>
    {
        return self.read_key_values_inner(key, out, true);
    }
    pub fn read_key_values_inner<V: bytemuck::Pod>(&mut self, key: K, out: &mut [V], size_check: bool) -> Result<(), NvsError<K, T>>
    {
        let size = size_of::<V>() * out.len();
        if size == 0 { return Ok(()); }
        
        let tv = match self.key_map.get_table_value(key)
        {
            Some(tv) => tv,
            None => return Err(NvsError::MissingKey(key))
        };
        
        // tv.get_size() <= C::PAGE_SIZE so too big is not a concern
        if size_check && tv.get_size() as usize != size// || size > C::PAGE_SIZE
        {
            return Err(NvsError::InconsistentSize(tv.get_size()));
        }
        
        // out is already aligned by READ_SIZE
        if size % C::READ_SIZE == 0
        {
            map_err!{self.partition.read(tv.get_address().0, bytemuck::cast_slice_mut(out))}?;
        }
        // otherwise reallocate with extra space for alignment
        else if out.len() == 1 // more efficient method for only one
        {
            let mut v: Padding<V, { C::READ_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
            // round up to READ_SIZE
            let size = round_up!(size, C::READ_SIZE);
            
            map_err!{self.partition.read(tv.get_address().0, v.as_bytes_mut(size))}?;
            
            out[0] = v.0;
            return Ok(());
        }
        else
        {
            // use cache as temporary data - it won't be in use at this time
            // data can't be bigger than page size
            let mut bytes = self.cache.get_or_alloc(C::PAGE_SIZE as usize);
            // round up to READ_SIZE
            let align_size = round_up!(size, C::READ_SIZE);
            
            map_err!{self.partition.read(tv.get_address().0, &mut bytes[..align_size])}?;
            // copy to output
            bytemuck::cast_slice_mut(out).copy_from_slice(&bytes[..size]);
            self.cache.return_cold(bytes);
        }
        
        return Ok(());
    }
}