use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, Nvs, NvsConstants, NvsKey, True};

impl<K: NvsKey, T: NorFlash + 'static, C: NvsConstants + 'static> Nvs<K, T, C>
    where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True,
        [(); T::WRITE_SIZE]: ,
        [(); T::READ_SIZE]: 
{
    pub(super) fn prepare_map(&mut self, ignore: K)
    {
        
    }
}