use core::marker::PhantomData;

use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, NvsConstants, NvsKey, True};

pub struct Nvs<K: NvsKey, T: NorFlash, C: NvsConstants>
{
    partition: T,
    _phantom: PhantomData<(C, K)>
}

impl<K: NvsKey, T: NorFlash, C: NvsConstants> Nvs<K, T, C>
    where CheckConst<{ T::ERASE_SIZE.is_power_of_two() }>: True
{
    pub fn write_key_value<V>(&mut self, key: K, value: V)
    {
        
    }
    
    pub fn read_key<V>(&mut self, key: K) -> V
    {
        todo!()
    }
}
