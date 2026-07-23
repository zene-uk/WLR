mod record_paging;
mod data_paging;
mod common;
mod page_address;
mod read;
mod write;

use core::{marker::PhantomData, panic};
use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;
use hashbrown::HashMap;

use crate::{NvsConstants, NvsError, NvsKey, cache::PageCache, data::{Address, Record}, key_map::KeyMap, map_err, nvs::page_address::PageAddresses, round_up, state::State};

pub(crate) type IgnoreTy<K, C, const KEY_COUNT: usize> = fn(K, &KeyMap<K, C, KEY_COUNT>, bool) -> bool;
pub(crate) trait Ignore<K: NvsKey, C: NvsConstants, const KEY_COUNT: usize>: Fn(K, &KeyMap<K, C, KEY_COUNT>, bool) -> bool {}
impl<K: NvsKey, C: NvsConstants, const KEY_COUNT: usize, T: Fn(K, &KeyMap<K, C, KEY_COUNT>, bool) -> bool> Ignore<K, C, KEY_COUNT> for T {}

pub struct Nvs<K: NvsKey, T: NorFlash, C: NvsConstants, const KEY_COUNT: usize>
{
    partition: T,
    key_map: KeyMap<K, C, KEY_COUNT>,
    /// the next addresses for data and records
    page_address: PageAddresses<C>,
    cache: PageCache,
    state: State<T, C>,
    write_queue: Option<HashMap<K, (&'static [u8], bool)>>,
    _phantom: PhantomData<C>
}
struct NvsShadow<'a, K: NvsKey, T: NorFlash, C: NvsConstants, F: Ignore<K, C, KEY_COUNT>, const KEY_COUNT: usize>
{
    partition: &'a mut T,
    key_map: &'a mut KeyMap<K, C, KEY_COUNT>,
    /// the next addresses for data and records
    page_address: &'a mut PageAddresses<C>,
    cache: &'a mut PageCache,
    state: &'a mut State<T, C>,
    ignore: F,
    _phantom: PhantomData<C>
}
impl<'a, K: NvsKey, T: NorFlash, C: NvsConstants + 'static, F: Ignore<K, C, KEY_COUNT>, const KEY_COUNT: usize> NvsShadow<'a, K, T, C, F, KEY_COUNT>
{
    const RECORD_OFFSET: usize = round_up!(size_of::<Record<C>>(), C::WRITE_SIZE);
    
    pub fn new(partition: &'a mut T,
        key_map: &'a mut KeyMap<K, C, KEY_COUNT>,
        page_address: &'a mut PageAddresses<C>,
        cache: &'a mut PageCache,
        state: &'a mut State<T, C>,
        ignore: F) -> NvsShadow<'a, K, T, C, F, KEY_COUNT>
    {
        return NvsShadow { partition, key_map, page_address, cache, state, ignore, _phantom: PhantomData };
    }
}

impl<K: NvsKey, T: NorFlash, C: NvsConstants + 'static, const KEY_COUNT: usize> Nvs<K, T, C, KEY_COUNT>
    where [(); C::WRITE_SIZE]:
{
    const RECORD_OFFSET: usize = round_up!(size_of::<Record<C>>(), C::WRITE_SIZE);
    
    fn as_shadow<'a, F: Ignore<K, C, KEY_COUNT>>(&'a mut self, ignore: F) -> NvsShadow<'a, K, T, C, F, KEY_COUNT>
    {
        return NvsShadow::new(&mut self.partition, &mut self.key_map, &mut self.page_address, &mut self.cache, &mut self.state, ignore);
    }
    
    fn check_consts(partition: &mut T)
    {
        // constants do not match
        if !(C::PAGE_SIZE as usize).is_multiple_of(T::ERASE_SIZE) || !C::WRITE_SIZE.is_multiple_of(T::WRITE_SIZE) ||
            !C::READ_SIZE.is_multiple_of(T::READ_SIZE) || K::COUNT != KEY_COUNT || partition.capacity() != (C::TOTAL_PAGES * C::PAGE_SIZE) as usize ||
        // invalid constants
            !T::ERASE_SIZE.is_power_of_two() || K::COUNT >= 0xFFFF || C::MAP_POST_PADDING <= C::MAPPING_MAX_RANGE ||
        // The maximum number of records does not leave any empty space in the map
            K::COUNT >= 1 + (C::MAPPING_MAX_RANGE as u32 * C::PAGE_SIZE) as usize / Self::RECORD_OFFSET ||
        // too many pages - not likely to ever be needed and helps reduce the cache memory footprint
            C::TOTAL_PAGES >= i32::MAX as u32 ||
        // page size too big - also helps reduce cache memory footprint
            C::PAGE_SIZE >= u16::MAX as u32
        {
            panic!();
        }
    }
    
    #[must_use]
    pub fn init(mut partition: T) -> Result<Self, NvsError<K, T>>
    {
        Self::check_consts(&mut partition);
        
        let state = State::init(&mut partition)?;
        let record_page = state.get_old_value();
        
        let mut key_map = KeyMap::new();
        
        let mut next_data_page = 0;
        let mut next_record_address = Address::u(0);
        let mut address_record = Address::u(0);
        
        let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(C::PAGE_SIZE as usize).assume_init() };
        // find all records
        for page in record_page..(record_page + C::MAPPING_MAX_RANGE as u32 - 1)
        {
            // read page
            map_err!{partition.read(Address::<C>::from_page(page as u32).0, &mut bytes)}?;
            
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
                        address_record = Address::from_page_offset(page, i as u32);
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
                        let record: Record<C> = 
                            *bytemuck::from_bytes(&bytes[i..(i+size_of::<Record<C>>())]);
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
        
        let page_address = PageAddresses { data: next_data_address, record: next_record_address,
            address_record, update_address_record: false };
        // add our allocation to the cold count
        let mut cache = PageCache::new();
        cache.cache_page(1, bytes, C::PAGE_SIZE as u16);
        cache.drop_all_pages();
        
        let mut res = Self { partition, key_map, page_address, cache, state,
            write_queue: Some(HashMap::with_capacity(K::COUNT)), _phantom: PhantomData };
        // get next page
        if run_next_page
        {
            let mut shadow = res.as_shadow(|_, _, _| false);
            shadow.next_data_page();
        }
        return Ok(res);
    }
    #[must_use]
    pub fn new(mut partition: T) -> Result<Self, NvsError<K, T>>
    {
        Self::check_consts(&mut partition);
        
        let mut key_map = KeyMap::new();
        key_map.initialise();
        
        let next_data_address = Address::from_page(C::STATE_PAGES as u32 + 1 + C::MAP_POST_PADDING as u32);
        let next_record_address = Address::from_page(C::STATE_PAGES as u32);
        let address_record = next_record_address;
        
        // page erasing is done in prepare functions
        // // erase initial record page
        // map_err!{partition.erase(next_data_address.0, next_data_address.0 + C::PAGE_SIZE)}?;
        
        // // erase initial data page
        // map_err!{partition.erase(next_record_address.0, next_record_address.0 + C::PAGE_SIZE)}?;
        
        let page_address = PageAddresses { data: next_data_address, record: next_record_address,
            address_record, update_address_record: true };
        
        let state = map_err!{State::new(&mut partition, 0)}?;
        return Ok(Self { partition, key_map, page_address, cache: PageCache::new(), state,
            write_queue: Some(HashMap::with_capacity(K::COUNT)), _phantom: PhantomData })
    }
}
