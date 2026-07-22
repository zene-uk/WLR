use core::{marker::PhantomData, panic};

use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{Nvs, NvsConstants, NvsKey, data::{Address, Record}, key_map::KeyMap, round_up, state::State};

impl<K: NvsKey, T: NorFlash, C: NvsConstants + 'static> Nvs<K, T, C>
{
    const RECORD_OFFSET: usize = round_up!(size_of::<Record<{ C::PAGE_SIZE }>>(), C::WRITE_SIZE);
    
    #[must_use]
    pub fn init(mut partition: T) -> Option<Self>
    {
        // constants do not match
        if T::ERASE_SIZE != C::PAGE_SIZE as usize || T::WRITE_SIZE != C::WRITE_SIZE ||
            T::READ_SIZE != C::READ_SIZE || K::COUNT != K::LEN
        {
            panic!();
        }
        
        // invalid constants
        if !T::ERASE_SIZE.is_power_of_two() || K::COUNT >= 0xFFFF
        {
            return None;
        }
        
        let state = State::init(&mut partition)?;
        let record_page = state.get_value();
        
        let mut key_map = KeyMap::new();
        
        let mut next_data_page = 0;
        let mut next_record_address = Address(0);
        
        let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(T::ERASE_SIZE).assume_init() };
        // find all records
        for page in record_page..(record_page + C::MAPPING_MAX_RANGE as u32)
        {
            // read page
            if partition.read(Address::<{ C::PAGE_SIZE }>::from_page(page as u32).0, &mut bytes).is_err()
            {
                return None;
            }
            
            for i in (0..T::ERASE_SIZE).step_by(Self::RECORD_OFFSET)
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
                            return None;
                        }
                    }
                }
            }
        }
        
        // create page info
        key_map.initialise();
        // TODO: change unwrap to use find next page instead
        let next_data_address = key_map.get_next_page_address(next_data_page).unwrap();
        
        return Some(Self { partition, key_map, next_data_address, next_record_address, state, _phantom: PhantomData });
    }
    #[must_use]
    pub fn new(mut partition: T) -> Option<Self>
    {
        let mut key_map = KeyMap::new();
        key_map.initialise();
        
        let next_data_address = Address::from_page(C::STATE_PAGES as u32 + C::MAP_POST_PADDING as u32);
        let next_record_address = Address::from_page(C::STATE_PAGES as u32);
        
        // erase initial record page
        if partition.erase(next_data_address.0, next_data_address.0 + T::ERASE_SIZE as u32).is_err()
        {
            return None;
        }
        
        // erase initial data page
        if partition.erase(next_record_address.0, next_record_address.0 + T::ERASE_SIZE as u32).is_err()
        {
            return None;
        }
        
        let state = State::new(&mut partition, 0)?;
        return Some(Self { partition, key_map, next_data_address, next_record_address, state, _phantom: PhantomData })
    }
}