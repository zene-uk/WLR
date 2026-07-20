use core::{marker::PhantomData, mem::MaybeUninit};

use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, NvsConstants, Padding, True, data::Address, round_up};

pub struct State<C: NvsConstants, const PAGE_SIZE: u32>
    where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
{
    address: Address<PAGE_SIZE>,
    value: u32,
    synced: bool,
    _phatom: PhantomData<C>
}

impl<C: NvsConstants + 'static, const PAGE_SIZE: u32> State<C, PAGE_SIZE>
    where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
{
    #[must_use]
    pub fn init<T: NorFlash>(partition: &mut T) -> Option<Self>
    {
        let offset = round_up!(size_of::<u32>(), T::WRITE_SIZE);
        
        for page in 0..C::STATE_PAGES
        {
            let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(T::ERASE_SIZE).assume_init() };
            // read page
            if partition.read(Address::<PAGE_SIZE>::from_page(page as u32).0, &mut bytes).is_err()
            {
                return None;
            }
            
            for i in (0..T::ERASE_SIZE).step_by(offset)
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
    pub fn new<T: NorFlash + 'static>(partition: &mut T, value: u32) -> Option<Self>
        where [(); T::WRITE_SIZE]:
    {
        // erase initial state page
        if partition.erase(0, T::ERASE_SIZE as u32).is_err()
        {
            return None;
        }
        
        // write initial value
        let offset = round_up!(size_of::<u32>(), T::WRITE_SIZE);
        let mut buffer: Padding<u32, { T::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        buffer.0 = value;
        if partition.write(0, &bytemuck::bytes_of(&buffer)[..offset]).is_err()
        {
            return None;
        }
        
        return Some(Self { address: Address(0), value, synced: true, _phatom: PhantomData });
    }
    
    pub fn sync_value<T: NorFlash + 'static>(&mut self, partition: &mut T) -> bool
        where [(); T::WRITE_SIZE]:
    {
        // no change
        if self.synced { return true; }
        
        let offset = round_up!(size_of::<u32>(), T::WRITE_SIZE);
        let mut buffer: Padding<u32, { T::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        // write all zeros to old address
        if partition.write(self.address.0, &bytemuck::bytes_of(&buffer)[..offset]).is_err()
        {
            return false;
        }
        
        let mut new_addr: Address<PAGE_SIZE> = (self.address.0 + offset as u32).into();
        if new_addr.get_page() >= C::STATE_PAGES as u32
        {
            new_addr = Address::from_page_offset(0, 0);
            // erase new page ready for data
            if partition.erase(0, T::ERASE_SIZE as u32).is_err()
            {
                return false;
            }
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
        if partition.write(new_addr.0, &bytemuck::bytes_of(&buffer)[..offset]).is_err()
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