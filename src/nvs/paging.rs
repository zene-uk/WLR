use core::{marker::PhantomData, mem::MaybeUninit};

use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsKey, Padding, data::{Address, Record}, key_map::{KeyMap, TableValue}, round_up, state::State};
// use crate::{CheckConst, True};

pub(super) struct NvsShadow<'a, K: NvsKey, T: NorFlash, C: NvsConstants, F: Fn(K) -> bool>
    // where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        //CheckConst<{ K::COUNT < 0xFFFF }>: True,
        // [(); T::WRITE_SIZE]: ,
        // [(); T::READ_SIZE]: ,
        // [(); { T::ERASE_SIZE as u32 } as usize]: ,
        // [(); K::COUNT]: 
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
    // where CheckConst<{ (T::ERASE_SIZE as u32).is_power_of_two() }>: True,
        //CheckConst<{ K::COUNT < 0xFFFF }>: True,
        // [(); T::WRITE_SIZE]: ,
        // [(); T::READ_SIZE]: ,
        // [(); { T::ERASE_SIZE as u32 } as usize]: ,
        // [(); K::COUNT]: 
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
    
    /// Ensures that the current record location is safe to write to
    pub fn prepare_map(&mut self) -> bool
    {
        // continue writing to current page map
        if !self.next_record_address.is_page_start() { return true; }
        let page = self.next_record_address.get_page();
        
        // need to move entries - do this before bringing back records to the front
        if !self.key_map.is_page_free(page)
        {
            self.move_page(page, true);
        }
        
        // dont need to move old records forward
        if page - self.state.get_value() < C::MAPPING_MAX_RANGE as u32
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
    pub fn move_page(&mut self, page: u32, erase: bool) -> bool
    {
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
            
            let addr = *self.next_data_address;
            let mut shadow_copy = NvsShadow::<'_, _, _, C, _>::new(self.partition, tr.key_map, self.next_data_address, self.next_record_address, self.state, &self.ignore);
            shadow_copy.write_entry_data(&[], &[]);
            
            let tv = tr.get_current_value();
            let rec_addr = *self.next_record_address;
            NvsShadow::<_, _, C, F>::write_record(self.partition, self.next_record_address, tv, addr);
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
        record: &TableValue<K, { C::PAGE_SIZE }>, new_addr: Address<{ C::PAGE_SIZE }>)
    {
        
    }
}