use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, data::Address, paging::NvsShadow};
// use crate::{CheckConst, True};

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
    // where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        // CheckConst<{ K::COUNT < 0xFFFF }>: True,
        // [(); T::WRITE_SIZE]: ,
        // [(); T::READ_SIZE]: ,
        // [(); { T::ERASE_SIZE as u32 } as usize]: ,
        // [(); K::COUNT]: 
{
    pub fn erase_page(&mut self, page: u32) -> bool
    {
        let offset = page * T::ERASE_SIZE as u32;
        return self.partition.erase(offset, offset + T::ERASE_SIZE as u32).is_ok();
    }
    pub fn read_page(&mut self, page: u32) -> Option<Box<[u8]>>
    {
        let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(T::ERASE_SIZE).assume_init() };
        
        if self.partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page as u32).0, &mut bytes).is_err()
        {
            return None;
        }
        
        return Some(bytes);
    }
}