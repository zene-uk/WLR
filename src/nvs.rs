use core::marker::PhantomData;

use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, NvsConstants, NvsKey, True, data::Address, key_map::KeyMap};

pub struct Nvs<K: NvsKey, T: NorFlash, C: NvsConstants>
    where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True
{
    partition: T,
    key_map: KeyMap<K, { T::ERASE_SIZE as u32 }>,
    next_data_address: Address<{ T::ERASE_SIZE as u32 }>,
    next_record_address: Address<{ T::ERASE_SIZE as u32 }>,
    _phantom: PhantomData<C>
}

impl<K: NvsKey, T: NorFlash, C: NvsConstants> Nvs<K, T, C>
    where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True
{
    pub fn write_key_value<V>(&mut self, key: K, value: V)
    {
        
    }
    
    pub fn read_key<V: bytemuck::Pod>(&mut self, key: K) -> Option<V>
        where [(); size_of::<V>()]: 
    {
        let tv = match self.key_map.get_table_value(key)
        {
            Some(tv) => tv,
            None => return None
        };
        
        if tv.get_size() as usize != size_of::<V>()
        {
            return None;
        }
        
        let mut bytes = [0u8; size_of::<V>()];
        if self.partition.read(tv.get_address().0, &mut bytes).is_err()
        {
            return None;
        }
        
        return Some(*bytemuck::from_bytes(&bytes));
    }
}
