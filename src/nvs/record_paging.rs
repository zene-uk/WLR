use core::mem::MaybeUninit;

use bytemuck::Zeroable;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsError, NvsKey, Padding, data::{Address, Record}, key_map::TableValue, map_err, nvs::NvsShadow, state::State};

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    /// Ensures that the current record location is safe to write to
    pub fn prepare_map(&mut self) -> Result<(), NvsError<K, T>>
    {
        // continue writing to current page map
        if !self.next_record_address.is_page_start() { return Ok(()); }
        
        // loop around
        let mut page = self.next_record_address.get_page();
        if Self::is_page_overrun(page)
        {
            *self.next_record_address = Address::from_page(C::STATE_PAGES as u32);
            page = C::STATE_PAGES as u32;
        }
        // no longer mutable
        let page = page;
        
        let back_map_page = self.state.get_new_value();
        let move_records = page - back_map_page >= C::MAPPING_MAX_RANGE as u32;
        if move_records
        {
            // set tmp value here so it moves along for recursions - but still leaves the old records
            self.state.set_tmp_value(back_map_page + 1);
        }
        
        // make sure our next data address is not writing to a page about to be moved
        // out tmp value change will now make this function true if in the new map padding pages
        while self.page_in_map_padding(self.next_data_address.get_page())
        {
            self.next_data_page();
        }
        
        // only move entries if we need to use the page - new values will never be written within the padding
        
        // need to move entries - do this before bringing back records to the front
        if !self.key_map.is_page_free(page)
        {
            // by setting erase to true, next_record_address is safe to write to when record calls are made
            self.move_data_page(page, true)?;
        }
        // dont recalculate move_records as if they needed moving, another prepare_map call would have dont it
        // dont need to move old records forward
        else
        {
            // page was free, erase ready for records
            self.erase_page(page)?;
        }
        
        // by this point it is safe to write to next_record_address
        if move_records
        {
            // move the unchanged out of range records
            self.move_map_page(back_map_page)?;
        }
        
        return Ok(());
    }
    
    pub fn move_map_page(&mut self, page: u32) -> Result<(), NvsError<K, T>>
    {
        // may need to rewrite next data page record as it could be moved without changing (very rare case)
        
        return Ok(());
    }
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// Only writes the record data, does not update the key_map
    #[must_use]
    pub fn write_record(partition: &mut T, state: &State<T, C>, nra: &mut Address<{ C::PAGE_SIZE }>,
        record: &TableValue<K, { C::PAGE_SIZE }>, new_addr: Address<{ C::PAGE_SIZE }>) -> Result<Address<{ C::PAGE_SIZE }>, NvsError<K, T>>
    {
        let mut write_data: Padding<Record<{ C::PAGE_SIZE }>, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        write_data.0 = record.to_record_new_addr(new_addr);
        
        let addr = *nra;
        map_err!{partition.write(addr.0, write_data.as_bytes(Self::RECORD_OFFSET))}?;
        
        // our old address is still in used pages
        let old_addr = record.get_record();
        if Self::is_in_old_map_page(state, old_addr.get_page())
        {
            // write zeros to old address
            write_data.0 = Record::zeroed();
            map_err!{partition.write(old_addr.0, write_data.as_bytes(Self::RECORD_OFFSET))}?;
        }
        
        // next address - bounds check in partition done on next prepare_map
        *nra += Self::RECORD_OFFSET as u32;
        return Ok(addr);
    }
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    /// 
    /// Only writes the records data, does not update the key_map
    #[must_use]
    pub fn write_new_record(&mut self, record: Record<{ C::PAGE_SIZE }>) -> Result<Address<{ C::PAGE_SIZE }>, NvsError<K, T>>
    {
        // sets the old record address to 0 and the unused page to 0 - so that it wont try to clear the old record as it does not exist
        let tv = TableValue::from_record(record, Address(0));
        return Self::write_record(self.partition, self.state, self.next_record_address, &tv, record.address);
    }
}