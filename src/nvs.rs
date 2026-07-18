use embedded_storage::nor_flash::NorFlash;

use crate::NvsConstants;

pub struct Nvs<T: NorFlash, C: NvsConstants>
{
    partition: T
}

impl<T: NorFlash, C: NvsConstants> Nvs<T, C>
{
    pub fn write_key_value<V>(&mut self, key: u16, value: V)
    {
        
    }
    
    pub fn read_key<V>(&mut self, key: u16) -> V
    {
        todo!()
    }
}