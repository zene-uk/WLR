use core::mem::MaybeUninit;

use bytemuck::Zeroable;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, Padding, data::{Address, Record}, key_map::TableValue, nvs::NvsShadow};

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    /// Ensures that the current record location is safe to write to
    pub fn prepare_map(&mut self) -> bool
    {
        // continue writing to current page map
        if !self.next_record_address.is_page_start() { return true; }
        let page = self.next_record_address.get_page();
        
        let back_map_page = self.state.get_value();
        let move_records = page - back_map_page >= C::MAPPING_MAX_RANGE as u32;
        if move_records
        {
            // make sure our next data address is not writing to a page about to be moved
            let last_padding_page = (page - C::MAPPING_MAX_RANGE as u32 + 1) + C::MAP_POST_PADDING as u32;
            // use current back page so we dont write over records that need to be moved
            let first_padding_page = back_map_page - C::MAP_PRE_PADDING as u32;
            while {
                let data_page = self.next_data_address.get_page();
                last_padding_page >= data_page && data_page >= first_padding_page
            }
            {
                self.next_data_page();
            }
        }
        
        // need to move entries - do this before bringing back records to the front
        if !self.key_map.is_page_free(page)
        {
            let unused_map_page = match move_records
            {
                true => back_map_page,
                false => u32::MAX
            };
            
            self.move_data_page(page, true, unused_map_page);
        }
        
        // dont need to move old records forward
        if !move_records
        {
            // erase next page ready for records
            return self.erase_page(page);
        }
        
        // TODO
        
        
        return true;
    }
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    pub fn write_record(partition: &mut T, nra: &mut Address<{ C::PAGE_SIZE }>,
        record: &TableValue<K, { C::PAGE_SIZE }>, new_addr: Address<{ C::PAGE_SIZE }>,
        unused_map_page: u32) -> bool
    {
        let addr = *nra;
        
        let mut write_data: Padding<Record<{ C::PAGE_SIZE }>, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        write_data.0 = record.to_record_new_addr(new_addr);
        
        if partition.write(addr.0, write_data.as_bytes(Self::RECORD_OFFSET)).is_err()
        {
            return false;
        }
        
        // our old address is still in used pages
        let old_addr = record.get_record();
        if old_addr.get_page() != unused_map_page
        {
            // write zeros to old address
            write_data.0 = Record::zeroed();
            if partition.write(old_addr.0, write_data.as_bytes(Self::RECORD_OFFSET)).is_err()
            {
                return false;
            }
        }
        
        // next address
        *nra = Address(addr.0 + Self::RECORD_OFFSET as u32);
        return true;
    }
}