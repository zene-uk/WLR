use core::{marker::PhantomData, mem::MaybeUninit};

use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;

use crate::{NvsConstants, NvsError, NvsKey, Padding, data::Address, map_err, round_up};

pub struct State<T: NorFlash, C: NvsConstants>
{
    address: Address<C>,
    value: u32,
    tmp_value: u32,
    synced: bool,
    _phatom: PhantomData<(C, T)>
}

impl<T: NorFlash, C: NvsConstants + 'static> State<T, C>
    where [(); C::WRITE_SIZE]:
{
    const OFFSET: usize = round_up!(size_of::<u32>(), C::WRITE_SIZE);
    
    #[must_use]
    pub fn init<K: NvsKey>(partition: &mut T) -> Result<Self, NvsError<K, T>>
    {
        let mut bytes: Box<[u8]> = unsafe { Box::new_zeroed_slice(C::PAGE_SIZE as usize).assume_init() };
        for page in 0..C::STATE_PAGES
        {
            // read page
            map_err!{partition.read(Address::<C>::from_page(page as u32).0, &mut bytes)}?;
            
            for i in (0..C::PAGE_SIZE as usize).step_by(Self::OFFSET)
            {
                let value: u32 = *bytemuck::from_bytes(&bytes[i..(i+size_of::<u32>())]);
                if value != 0
                {
                    let address = Address::from_page_offset(page as u32, i as u32);
                    return Ok(Self { address, value, tmp_value: value, synced: true, _phatom: PhantomData });
                }
            }
        }
        
        return Err(NvsError::MissingState);
    }
    #[must_use]
    pub fn new(partition: &mut T, value: u32) -> Result<Self, T::Error>
    {
        // erase initial state page
        partition.erase(0, C::PAGE_SIZE as u32)?;
        
        // write initial value
        let mut buffer: Padding<u32, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        buffer.0 = value;
        partition.write(0, buffer.as_bytes(Self::OFFSET))?;
        
        // address is the current location, next one will be calculated when needed
        return Ok(Self { address: Address::u(0), value, tmp_value: value, synced: true, _phatom: PhantomData });
    }
    
    pub fn sync_value(&mut self, partition: &mut T) -> Result<(), T::Error>
    {
        // no change
        if self.synced { return Ok(()); }
        
        let mut buffer: Padding<u32, { C::WRITE_SIZE }> = unsafe { MaybeUninit::zeroed().assume_init() };
        // write all zeros to old address
        partition.write(self.address.0, buffer.as_bytes(Self::OFFSET))?;
        
        let mut new_addr = self.address + Self::OFFSET as u32;
        if new_addr.get_page() >= C::STATE_PAGES as u32
        {
            new_addr = Address::from_page_offset(0, 0);
        }
        // change in page - C::PAGE_SIZE should be multiple of offset
        if new_addr.is_page_start()
        {
            // erase new page ready for data
            partition.erase(new_addr.0, new_addr.0 + C::PAGE_SIZE)?;
        }
        
        // write new value
        buffer.0 = self.value;
        partition.write(new_addr.0, buffer.as_bytes(Self::OFFSET))?;
        self.address = new_addr;
        self.synced = true;
        return Ok(());
    }
    
    #[inline]
    #[must_use]
    pub fn get_new_value(&self) -> u32
    {
        return self.tmp_value;
    }
    #[inline]
    #[must_use]
    pub fn get_old_value(&self) -> u32
    {
        return self.value;
    }
    #[inline]
    pub fn set_tmp_value(&mut self, value: u32)
    {
        if self.value == value { return; }
        
        self.synced = false;
        self.tmp_value = value;
    }
    pub fn shift_tmp_to_value(&mut self)
    {
        self.value = self.tmp_value;
    }
}