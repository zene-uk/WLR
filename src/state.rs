use core::{marker::PhantomData, mem::MaybeUninit};

use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{CheckConst, NvsConstants, Padding, True, data::Address, round_up};

pub struct State<C: NvsConstants, const PAGE_SIZE: u32>
    where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
{
    address: Address<PAGE_SIZE>,
    value: Address<PAGE_SIZE>,
    _phatom: PhantomData<C>
}

impl<C: NvsConstants + 'static, const PAGE_SIZE: u32> State<C, PAGE_SIZE>
    where CheckConst<{ PAGE_SIZE.is_power_of_two() }>: True
{
    pub fn init<T: NorFlash>(partition: &mut T) -> Option<Self>
    {
        let offset = round_up!(size_of::<u32>(), T::WRITE_SIZE);
        
        for p in 0..C::STATE_PAGES
        {
            let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(T::ERASE_SIZE).assume_init() };
            // read page
            if partition.read(Address::<PAGE_SIZE>::from_page(p as u32).0, &mut bytes).is_err()
            {
                return None;
            }
            
            for i in (0..T::ERASE_SIZE).step_by(offset)
            {
                let value: u32 = *bytemuck::from_bytes(&bytes[i..(i+4)]);
                if value != 0
                {
                    let address = Address::from_page_offset(p as u32, i as u32);
                    return Some(Self { address, value: Address(value), _phatom: PhantomData });
                }
            }
        }
        
        return None;
    }
    
    pub fn update_value<T: NorFlash + 'static>(&mut self, partition: &mut T, value: Address<PAGE_SIZE>) -> bool
        where [(); T::WRITE_SIZE]:
    {
        // no change
        if self.value == value
        {
            return true;
        }
        
        let offset = round_up!(size_of::<u32>(), T::WRITE_SIZE);
        let mut buffer: Padding<u32, { T::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        // write all zeros to old address
        if partition.write(self.address.0, &bytemuck::bytes_of(&buffer)[..offset]).is_err()
        {
            return false;
        }
        
        let new_addr = self.address.0 + offset as u32;
        
        // write new value
        buffer.0 = value.0;
        if partition.write(new_addr, &bytemuck::bytes_of(&buffer)[..offset]).is_err()
        {
            return false;
        }
        self.value = value;
        self.address = new_addr.into();
        return true;
    }
}