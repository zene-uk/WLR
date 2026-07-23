use crate::{NvsConstants, data::Address};

pub struct PageAddresses<C: NvsConstants>
{
    pub data: Address<C>,
    pub record: Address<C>,
    /// The address of the last written page_address.data record
    pub address_record: Address<C>,
    pub update_address_record: bool
}

impl<C: NvsConstants> PageAddresses<C>
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