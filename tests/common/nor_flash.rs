use embedded_storage::nor_flash::{ErrorType, NorFlash, NorFlashError, ReadNorFlash};

pub struct NorPage
{
    data: Box<[u8]>,
    erase_count: usize
}

pub struct NorFlashEm<const PAGE_SIZE: usize>
{
    pages: Box<[NorPage]>
}

impl<const PAGE_SIZE: usize> NorFlashEm<PAGE_SIZE>
{
    #[must_use]
    pub fn new(num_pages: usize) -> Self
    {
        let iter = std::iter::repeat_n((), num_pages).map(|_|
        {
            NorPage { data: unsafe {
                Box::new_uninit_slice(PAGE_SIZE).assume_init()
            }, erase_count: 0 }
        });
        return Self { pages: Box::from_iter(iter) };
    }
    #[must_use]
    pub fn get_stats(&self) -> (usize, f64, usize)
    {
        let mut total = 0;
        let mut min = usize::MAX;
        let mut max = 0;
        
        for page in &self.pages
        {
            let ec = page.erase_count;
            total += ec;
            if ec > max { max = ec; }
            if ec < min { min = ec; }
        }
        
        return (min, total as f64 / self.pages.len() as f64, max);
    }
}

#[derive(Debug)]
pub struct EmError;
impl NorFlashError for EmError
{
    fn kind(&self) -> embedded_storage::nor_flash::NorFlashErrorKind
    {
        return embedded_storage::nor_flash::NorFlashErrorKind::Other;
    }
}
impl<const PAGE_SIZE: usize> ErrorType for NorFlashEm<PAGE_SIZE>
{
    type Error = EmError;
}
impl<const PAGE_SIZE: usize> ReadNorFlash for NorFlashEm<PAGE_SIZE>
{
    const READ_SIZE: usize = 4;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error>
    {
        let offset = offset as usize;
        if offset % Self::READ_SIZE != 0 || bytes.len() % Self::READ_SIZE != 0
        {
            return Err(EmError);
        }
        
        let mut page = offset / PAGE_SIZE;
        let mut page_offset = offset % PAGE_SIZE;
        
        let mut read = 0;
        while read < bytes.len()
        {
            let page_remaining = PAGE_SIZE - page_offset;
            let bytes_remaining = bytes.len() - read;
            
            let amount = page_remaining.min(bytes_remaining);
            
            let page_data = self.pages.get(page).ok_or(EmError)?;
            let read_data = page_data.data.get(page_offset..(page_offset + amount)).ok_or(EmError)?;
            bytes[read..(read + amount)].copy_from_slice(read_data);
            read += amount;
            page += 1;
            page_offset = 0;
        }
        
        return Ok(());
    }

    fn capacity(&self) -> usize
    {
        return self.pages.len() * PAGE_SIZE;
    }
}
impl<const PAGE_SIZE: usize> NorFlash for NorFlashEm<PAGE_SIZE>
{
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = PAGE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error>
    {
        let offset = from as usize;
        let len = (to - from) as usize;
        if offset % Self::ERASE_SIZE != 0 || len % Self::ERASE_SIZE != 0
        {
            return Err(EmError);
        }
        
        let mut page = offset / PAGE_SIZE;
        
        let mut read = 0;
        while read < len
        {
            let page_data = self.pages.get_mut(page).ok_or(EmError)?;
            // fill data
            page_data.data.fill(0xFF);
            page_data.erase_count += 1;
            read += PAGE_SIZE;
            page += 1;
        }
        
        return Ok(());
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error>
    {
        let offset = offset as usize;
        if offset % Self::READ_SIZE != 0 || bytes.len() % Self::READ_SIZE != 0
        {
            return Err(EmError);
        }
        
        let mut page = offset / PAGE_SIZE;
        let mut page_offset = offset % PAGE_SIZE;
        
        let mut read = 0;
        while read < bytes.len()
        {
            let page_remaining = PAGE_SIZE - page_offset;
            let bytes_remaining = bytes.len() - read;
            
            let amount = page_remaining.min(bytes_remaining);
            
            let page_data = self.pages.get_mut(page).ok_or(EmError)?;
            let read_data = page_data.data.get_mut(page_offset..(page_offset + amount)).ok_or(EmError)?;
            read_data.copy_from_slice(&bytes[read..(read + amount)]);
            read += amount;
            page += 1;
            page_offset = 0;
        }
        
        return Ok(());
    }
}