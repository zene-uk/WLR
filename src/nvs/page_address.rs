use crate::data::Address;

pub struct PageAddresses<const PAGE_SIZE: u32>
{
    pub data: Address<PAGE_SIZE>,
    pub record: Address<PAGE_SIZE>,
    /// The address of the last written next_data_address record
    pub address_record: Address<PAGE_SIZE>,
    pub update_address_record: bool
}

impl<const PAGE_SIZE: u32> PageAddresses<PAGE_SIZE>
{
    #[inline]
    #[must_use]
    pub fn get_data_page(&self) -> u32
    {
        return self.data.get_page();
    }
    #[inline]
    #[must_use]
    pub fn get_record_page(&self) -> u32
    {
        return self.record.get_page();
    }
}