use core::{marker::PhantomData, mem::MaybeUninit};

use alloc::boxed::Box;
use bytemuck::Zeroable;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, Padding, data::{Address, Record}, key_map::{KeyMap, TableValue}, round_up, state::State};

pub(super) struct NvsShadow<'a, K: NvsKey, T: NorFlash, C: NvsConstants, F: Fn(K) -> bool>
{
    pub partition: &'a mut T,
    pub key_map: &'a mut KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
    pub next_data_address: &'a mut Address<{ C::PAGE_SIZE }>,
    pub next_record_address: &'a mut Address<{ C::PAGE_SIZE }>,
    pub state: &'a mut State<T, C, { C::PAGE_SIZE }>,
    pub ignore: F,
    _phantom: PhantomData<C>
}

impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Fn(K) -> bool> NvsShadow<'a, K, T, C, F>
{
    const RECORD_OFFSET: usize = round_up!(size_of::<Record<{ C::PAGE_SIZE }>>(), T::WRITE_SIZE);
    
    pub fn new(partition: &'a mut T,
        key_map: &'a mut KeyMap<K, { C::PAGE_SIZE }, { C::WRITE_SIZE }>,
        next_data_address: &'a mut Address<{ C::PAGE_SIZE }>,
        next_record_address: &'a mut Address<{ C::PAGE_SIZE }>,
        state: &'a mut State<T, C, { C::PAGE_SIZE }>,
        ignore: F) -> NvsShadow<'a, K, T, C, F>
    {
        return NvsShadow { partition, key_map, next_data_address, next_record_address, state, ignore, _phantom: PhantomData };
    }
    
    fn next_data_page(&mut self)
    {
        let mut page = self.next_data_address.get_page() + 1;
        let back_map_page = self.state.get_value();
        let last_padding_page = back_map_page + C::MAP_POST_PADDING as u32;
        if last_padding_page >= page &&
            back_map_page - C::MAP_PRE_PADDING as u32 <= page
        {
            page = last_padding_page + 1;
        }
        
        *self.next_data_address = Address::from_page(page);
    }
    
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
            
            self.move_page(page, true, unused_map_page);
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
    /// 
    /// This function will also call `prepare_map` ready for the next record write
    pub fn move_page(&mut self, page: u32, erase: bool, unused_map_page: u32) -> bool
    {
        // need to always reallocate due to potential recursion
        let page_data = match self.read_page(page)
        {
            Some(d) => d,
            None => return false
        };
        if erase
        {
            // incase we need to prepare for new records
            self.erase_page(page);
        }
        
        let iter = match self.key_map.get_page_values(page)
        {
            Some(i) => i,
            None => return false
        };
        // iterate through items that need moving
        for tr in iter
        {
            // skip ignore
            if (self.ignore)(tr.get_key()) { continue; }
            // consider overflow data
            
            let tv = tr.get_current_value();
            let (data, extra_data) = match Self::get_tv_data(self.partition, tv, &page_data, page)
            {
                Some(v) => v,
                None => return false
            };
            
            let addr = *self.next_data_address;
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.next_data_address, self.next_record_address, self.state, &self.ignore);
            shadow_copy.write_entry_data(data, &extra_data);
            
            let tv = tr.get_current_value();
            let rec_addr = *self.next_record_address;
            if !NvsShadow::<_, _, C, F>::write_record(self.partition, self.next_record_address, tv, addr, unused_map_page)
            {
                return false;
            }
            let size = tv.get_size();
            tr.key_map.update_record(tr.get_key(), rec_addr, addr, size);
            
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.next_data_address, self.next_record_address, self.state, &self.ignore);
            shadow_copy.prepare_map();
        }
        
        return true;
    }
    
    
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    pub fn write_entry_data(&mut self, data1: &[u8], data2: &[u8])
    {
        
    }
    /// It must be ok to write to next_record_address and update its value,
    /// i.e. `prepare_map` needs to have been called for the first write.
    fn write_record(partition: &mut T, nra: &mut Address<{ C::PAGE_SIZE }>,
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
    fn get_tv_data<'b>(partition: &mut T, tv: &TableValue<K, { C::PAGE_SIZE }>, page_data: &'b [u8], page: u32) -> Option<(&'b [u8], Box<[u8]>)>
    {
        let overflow_size = tv.get_overflow_size(C::WRITE_SIZE as u32) as usize;
        let data_footprint = tv.get_data_footprint(C::WRITE_SIZE as u32) as usize;
        let offset = tv.get_address().get_page_offset() as usize;
        let data_end = offset + data_footprint - overflow_size;
        
        // read overflow if necessary
        let extra_data = match overflow_size
        {
            0 => unsafe { Box::<[u8]>::new_uninit_slice(0).assume_init() },
            _ =>
            {
                let mut data = unsafe { Box::<[u8]>::new_uninit_slice(overflow_size).assume_init() };
                if partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page + 1).0, &mut data).is_err()
                {
                    return None;
                }
                data
            }
        };
        
        return Some((&page_data[offset..data_end], extra_data));
    }
}