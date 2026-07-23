use core::{marker::PhantomData, ops::{Add, AddAssign}};

use crate::{NvsConstants, NvsKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address<C: NvsConstants>(pub u32, PhantomData<C>);

impl<C: NvsConstants> Address<C>
{
    const PAGE_BIT: u32 = C::PAGE_SIZE.ilog2();
    const PAGE_BIT_SELECT: u32 = C::PAGE_SIZE - 1;
    
    // #[inline]
    // #[must_use]
    // pub fn get_partition_offset(self) -> u32
    // {
    //     return self.0;
    // }
    
    pub fn u(addr: u32) -> Self
    {
        return Self(addr, PhantomData);
    }
    
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
        return Self(page << Self::PAGE_BIT, PhantomData);
    }
    #[inline]
    #[must_use]
    pub fn from_page_offset(page: u32, offset: u32) -> Self
    {
        return Self((page << Self::PAGE_BIT) + (offset & Self::PAGE_BIT_SELECT), PhantomData);
    }
    
    #[inline]
    #[must_use]
    /// returns the remaining space including this address
    pub fn get_remaining_space(self) -> u32
    {
        return (C::PAGE_SIZE - self.get_page_offset()) + 1;
    }
    #[inline]
    #[must_use]
    pub fn is_page_start(self) -> bool
    {
        return self.get_page_offset() == 0;
    }
}

impl<C: NvsConstants> From<u32> for Address<C>
{
    fn from(value: u32) -> Self
    {
        return Self(value, PhantomData);
    }
}
impl<C: NvsConstants> Add<u32> for Address<C>
{
    type Output = Self;

    fn add(self, rhs: u32) -> Self
    {
        return Self(self.0 + rhs, PhantomData);
    }
}
impl<C: NvsConstants> AddAssign<u32> for Address<C>
{
    fn add_assign(&mut self, rhs: u32)
    {
        self.0 += rhs;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Record<C: NvsConstants>
{
    pub size: u16,
    pub key: u16,
    pub address: Address<C>
}
unsafe impl<C: NvsConstants> bytemuck::Zeroable for Record<C> {}
unsafe impl<C: NvsConstants> bytemuck::Pod for Record<C> {}
impl<C: NvsConstants> Record<C>
{
    #[inline]
    #[must_use]
    pub fn get_key<K: NvsKey>(&self) -> K
    {
        return K::from_key_value(self.key);
    }
}