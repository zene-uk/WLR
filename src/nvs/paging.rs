use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, Nvs, NvsConstants, NvsKey, True};

impl<K: NvsKey, T: NorFlash + 'static, C: NvsConstants + 'static> Nvs<K, T, C>
    where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        CheckConst<{ K::COUNT < 0xFFFF }>: True,
        [(); T::WRITE_SIZE]: ,
        [(); T::READ_SIZE]: 
{
    fn record_can_be_next_page(&self) -> bool
    {
        let page = self.next_record_address.get_page();
        // there are entries on the page
        if !self.key_map.is_page_free(page) { return false; }
        let map_start = self.state.get_value();
        
        return page - map_start < C::MAPPING_MAX_RANGE as u32;
    }
    pub(super) fn prepare_map(&mut self, ignore: K) -> bool
    {
        // continue writing to current page map
        if !self.next_record_address.is_page_start() { return true; }
        if self.record_can_be_next_page()
        {
            // erase next page ready for records
            return self.erase_page(self.next_record_address.get_page());
        }
        
        // TODO
        
        return true;
    }
}