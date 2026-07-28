mod nor_flash;
use std::println;

use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use enum_table::Enumable;
pub use nor_flash::*;
use wlr::{NvsConstants, NvsKey};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Key<const SIZE: usize>(u16);
impl<const SIZE: usize> From<u16> for Key<SIZE>
{
    fn from(value: u16) -> Self
    {
        return Key(value % SIZE as u16);
    }
}
impl<const SIZE: usize> Into<u16> for Key<SIZE>
{
    fn into(self) -> u16
    {
        return self.0;
    }
}
impl<const SIZE: usize> Enumable for Key<SIZE>
{
    const VARIANTS: &'static [Self] = &{
        let mut array = [Key(0); SIZE];
        let mut i = 0;
        
        while i < SIZE
        {
            array[i] = Key(i as u16);
            i += 1;
        }
        
        array
    };
    const COUNT: usize = SIZE;
    
    fn variant_index(&self) -> usize
    {
        return self.0 as usize;
    }
}
impl<const SIZE: usize> NvsKey for Key<SIZE>
{
    // #[type_const]
    // const LEN: usize = SIZE;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Constants;
impl NvsConstants for Constants
{
    const MAPPING_MAX_RANGE: u8 = 4;
    const MAP_PRE_PADDING: u8 = 1;
    const STATE_PAGES: u8 = 2;
    const MAP_POST_PADDING: u8 = 6;

    const TOTAL_PAGES: u32 = 1024;
    // #[type_const]
    const PAGE_SIZE: u32 = 4096;
    // #[type_const]
    const WRITE_SIZE: usize = 4;
    // #[type_const]
    const READ_SIZE: usize = 4;
}

#[derive(Debug)]
pub struct DebugFlash<T: NorFlash>
{
    inner: T
}
impl<T: NorFlash> DebugFlash<T>
{
    pub fn new(inner: T) -> Self
    {
        return Self { inner };
    }
}
impl<T: NorFlash> ErrorType for DebugFlash<T>
{
    type Error = T::Error;
}
impl<T: NorFlash> ReadNorFlash for DebugFlash<T>
{
    const READ_SIZE: usize = T::READ_SIZE;
    
    #[inline]
    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error>
    {
        let len = bytes.len();
        println!("Reading {len} bytes at {offset}");
        return self.inner.read(offset, bytes);
    }
    
    #[inline]
    fn capacity(&self) -> usize
    {
        return self.inner.capacity();
    }
}
impl<T: NorFlash> NorFlash for DebugFlash<T>
{
    const WRITE_SIZE: usize = T::WRITE_SIZE;
    const ERASE_SIZE: usize = T::ERASE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error>
    {
        let len = to - from;
        println!("Erasing {len} bytes at {from}");
        return self.inner.erase(from, to);
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error>
    {
        let len = bytes.len();
        println!("Writing {len} bytes to {offset}");
        return self.inner.write(offset, bytes);
    }
}