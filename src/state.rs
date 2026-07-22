use core::{marker::PhantomData, mem::MaybeUninit};

use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, Padding, data::Address, round_up};
// use crate::{CheckConst, True};

pub struct State<T: NorFlash, C: NvsConstants, const PAGE_SIZE: u32>
    // where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
{
    address: Address<PAGE_SIZE>,
    value: u32,
    synced: bool,
    _phatom: PhantomData<(C, T)>
}

impl<T: NorFlash, C: NvsConstants + 'static, const PAGE_SIZE: u32> State<T, C, PAGE_SIZE>
    // where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
    // where [(); T::WRITE_SIZE]:
{
    const OFFSET: usize = round_up!(size_of::<u32>(), T::WRITE_SIZE);
    
    #[must_use]
    pub fn init(partition: &mut T) -> Option<Self>
    {
        let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(T::ERASE_SIZE).assume_init() };
        for page in 0..C::STATE_PAGES
        {
            // read page
            if partition.read(Address::<PAGE_SIZE>::from_page(page as u32).0, &mut bytes).is_err()
            {
                return None;
            }
            
            for i in (0..T::ERASE_SIZE).step_by(Self::OFFSET)
            {
                let value: u32 = *bytemuck::from_bytes(&bytes[i..(i+size_of::<u32>())]);
                if value != 0
                {
                    let address = Address::from_page_offset(page as u32, i as u32);
                    return Some(Self { address, value, synced: true, _phatom: PhantomData });
                }
            }
        }
        
        return None;
    }
    #[must_use]
    pub fn new(partition: &mut T, value: u32) -> Option<Self>
    {
        // erase initial state page
        if partition.erase(0, T::ERASE_SIZE as u32).is_err()
        {
            return None;
        }
        
        // write initial value
        let mut buffer: Padding<u32, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        buffer.0 = value;
        if partition.write(0, buffer.as_bytes(Self::OFFSET)).is_err()
        {
            return None;
        }
        
        return Some(Self { address: Address(0), value, synced: true, _phatom: PhantomData });
    }
    
    pub fn sync_value(&mut self, partition: &mut T) -> bool
    {
        // no change
        if self.synced { return true; }
        
        let mut buffer: Padding<u32, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        // write all zeros to old address
        if partition.write(self.address.0, buffer.as_bytes(Self::OFFSET)).is_err()
        {
            return false;
        }
        
        let mut new_addr: Address<PAGE_SIZE> = (self.address.0 + Self::OFFSET as u32).into();
        if new_addr.get_page() >= C::STATE_PAGES as u32
        {
            new_addr = Address::from_page_offset(0, 0);
        }
        // change in page - T::ERASE_SIZE should be multiple of offset
        if self.address.is_page_start()
        {
            // erase new page ready for data
            if partition.erase(new_addr.0, new_addr.0 + T::ERASE_SIZE as u32).is_err()
            {
                return false;
            }
        }
        
        // write new value
        buffer.0 = self.value;
        if partition.write(new_addr.0, buffer.as_bytes(Self::OFFSET)).is_err()
        {
            return false;
        }
        self.address = new_addr;
        self.synced = true;
        return true;
    }
    
    #[inline]
    #[must_use]
    pub fn get_value(&self) -> u32
    {
        return self.value;
    }
    #[inline]
    pub fn set_value(&mut self, value: u32)
    {
        if self.value == value { return; }
        
        self.synced = false;
        self.value = value;
    }
}