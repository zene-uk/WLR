#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address<const PAGE_BIT: u32>(pub u32);

impl<const PAGE_BIT: u32> Address<PAGE_BIT>
{
    const PAGE_BIT_SELECT: u32 = (1 << PAGE_BIT) - 1;
    
    #[inline]
    #[must_use]
    pub fn get_partition_offset(self) -> u32
    {
        return self.0;
    }
    
    #[inline]
    #[must_use]
    pub fn get_page(self) -> u32
    {
        return self.0 >> PAGE_BIT;
    }
    
    #[inline]
    #[must_use]
    pub fn get_page_offset(self) -> u32
    {
        return self.0 & Self::PAGE_BIT_SELECT;
    }
}

impl<const PAGE_BIT: u32> From<u32> for Address<PAGE_BIT>
{
    fn from(value: u32) -> Self
    {
        return Self(value);
    }
}

#[repr(C)]
pub struct Record<const PAGE_BIT: u32>
{
    size: u16,
    key: u16,
    address: Address<PAGE_BIT>
}