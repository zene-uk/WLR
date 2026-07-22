use core::ops::{Add, AddAssign};

use crate::NvsKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address<const PAGE_SIZE: u32>(pub u32);

impl<const PAGE_SIZE: u32> Address<PAGE_SIZE>
{
    const PAGE_BIT: u32 = PAGE_SIZE.ilog2();
    const PAGE_BIT_SELECT: u32 = PAGE_SIZE - 1;
    
    // #[inline]
    // #[must_use]
    // pub fn get_partition_offset(self) -> u32
    // {
    //     return self.0;
    // }
    
    #[inline]
    #[must_use]
    pub fn get_page(self) -> u32
    {
        return self.0 >> Self::PAGE_BIT;
    }
    
    #[inline]
    #[must_use]
    pub fn get_page_offset(self) -> u32
    {
        return self.0 & Self::PAGE_BIT_SELECT;
    }
    
    #[inline]
    #[must_use]
    pub fn from_page(page: u32) -> Self
    {
        return Self(page << Self::PAGE_BIT);
    }
    #[inline]
    #[must_use]
    pub fn from_page_offset(page: u32, offset: u32) -> Self
    {
        return Self((page << Self::PAGE_BIT) + (offset & Self::PAGE_BIT_SELECT));
    }
    
    #[inline]
    #[must_use]
    /// returns the remaining space including this address
    pub fn get_remaining_space(self) -> u32
    {
        return (PAGE_SIZE - self.get_page_offset()) + 1;
    }
    #[inline]
    #[must_use]
    pub fn is_page_start(self) -> bool
    {
        return self.get_page_offset() == 0;
    }
}

impl<const PAGE_SIZE: u32> From<u32> for Address<PAGE_SIZE>
{
    fn from(value: u32) -> Self
    {
        return Self(value);
    }
}
impl<const PAGE_SIZE: u32> Add<u32> for Address<PAGE_SIZE>
{
    type Output = Self;

    fn add(self, rhs: u32) -> Self
    {
        return Self(self.0 + rhs);
    }
}
impl<const PAGE_SIZE: u32> AddAssign<u32> for Address<PAGE_SIZE>
{
    fn add_assign(&mut self, rhs: u32)
    {
        self.0 += rhs;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Record<const PAGE_SIZE: u32>
{
    pub size: u16,
    pub key: u16,
    pub address: Address<PAGE_SIZE>
}
unsafe impl<const PAGE_SIZE: u32> bytemuck::Zeroable for Record<PAGE_SIZE> {}
unsafe impl<const PAGE_SIZE: u32> bytemuck::Pod for Record<PAGE_SIZE> {}
impl<const PAGE_SIZE: u32> Record<PAGE_SIZE>
{
    #[inline]
    #[must_use]
    pub fn get_key<K: NvsKey>(&self) -> K
    {
        return K::from_key_value(self.key);
    }
}