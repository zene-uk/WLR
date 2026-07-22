use core::{marker::PhantomData, panic};

use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{Nvs, NvsConstants, NvsError, NvsKey, data::{Address, Record}, key_map::KeyMap, map_err, round_up, state::State};

impl<K: NvsKey, T: NorFlash, C: NvsConstants + 'static> Nvs<K, T, C>
{
    const RECORD_OFFSET: usize = round_up!(size_of::<Record<{ C::PAGE_SIZE }>>(), C::WRITE_SIZE);
    
    #[must_use]
    pub fn init(mut partition: T) -> Result<Self, NvsError<K, T>>
    {
        // constants do not match
        if (C::PAGE_SIZE as usize).is_multiple_of(T::ERASE_SIZE) || C::WRITE_SIZE.is_multiple_of(T::WRITE_SIZE) ||
            C::READ_SIZE.is_multiple_of(T::READ_SIZE) || K::COUNT != K::LEN || partition.capacity() != (C::TOTAL_PAGES * C::PAGE_SIZE) as usize ||
        // invalid constants
            !T::ERASE_SIZE.is_power_of_two() || K::COUNT >= 0xFFFF || C::MAP_POST_PADDING <= C::MAPPING_MAX_RANGE ||
        // The maximum number of records does not leave any empty space in the map
            K::COUNT >= 1 + (C::MAPPING_MAX_RANGE as u32 * C::PAGE_SIZE) as usize / Self::RECORD_OFFSET
        {
            panic!();
        }
        
        let state = State::init(&mut partition)?;
        let record_page = state.get_old_value();
        
        let mut key_map = KeyMap::new();
        
        let mut next_data_page = 0;
        let mut next_record_address = Address(0);
        
        let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(C::PAGE_SIZE as usize).assume_init() };
        // find all records
        for page in record_page..(record_page + C::MAPPING_MAX_RANGE as u32 - 1)
        {
            // read page
            map_err!{partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page as u32).0, &mut bytes)}?;
            
            for i in (0..C::PAGE_SIZE as usize).step_by(Self::RECORD_OFFSET)
            {
                let key: u32 = *bytemuck::from_bytes(&bytes[i..(i+size_of::<u32>())]);
                match key
                {
                    // stores extra value - last one found is that actual data
                    // means we dont have to override old ones with zeros
                    0xFFFF_0000 =>
                    {
                        // read next u32
                        let value: u32 = *bytemuck::from_bytes(&bytes[(i+size_of::<u32>())..(i+size_of::<u32>()+size_of::<u32>())]);
                        next_data_page = value;
                    },
                    // unset data - no more records
                    0xFFFF_FFFF =>
                    {
                        next_record_address = Address::from_page_offset(page, i as u32);
                    }
                    // empty record
                    0 => continue,
                    // record contains data
                    _ =>
                    {
                        let record: Record<{ C::PAGE_SIZE }> = 
                            *bytemuck::from_bytes(&bytes[i..(i+size_of::<Record<{ C::PAGE_SIZE }>>())]);
                        let ra = Address::from_page_offset(page, i as u32);
                        if !key_map.add_value(record, ra)
                        {
                            return Err(NvsError::DuplicateKey(record.get_key()));
                        }
                    }
                }
            }
        }
        
        // create page info
        key_map.initialise();
        
        let mut run_next_page = false;
        let next_data_address = match key_map.get_page_next_address(next_data_page)
        {
            Some(a) => a,
            None =>
            {
                run_next_page = true;
                Address::from_page(next_data_page)
            }
        };
        
        let mut res = Self { partition, key_map, next_data_address, next_record_address, state, _phantom: PhantomData };
        // get next page
        if run_next_page
        {
            let mut shadow = res.as_shadow(|_| false);
            shadow.next_data_page();
        }
        return Ok(res);
    }
    #[must_use]
    pub fn new(mut partition: T) -> Result<Self, NvsError<K, T>>
    {
        let mut key_map = KeyMap::new();
        key_map.initialise();
        
        let next_data_address = Address::from_page(C::STATE_PAGES as u32 + 1 + C::MAP_POST_PADDING as u32);
        let next_record_address = Address::from_page(C::STATE_PAGES as u32);
        
        // page erasing is done in prepare functions
        // // erase initial record page
        // map_err!{partition.erase(next_data_address.0, next_data_address.0 + C::PAGE_SIZE)}?;
        
        // // erase initial data page
        // map_err!{partition.erase(next_record_address.0, next_record_address.0 + C::PAGE_SIZE)}?;
        
        let state = map_err!{State::new(&mut partition, 0)}?;
        return Ok(Self { partition, key_map, next_data_address, next_record_address, state, _phantom: PhantomData })
    }
}