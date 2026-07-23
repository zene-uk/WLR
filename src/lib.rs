#![no_std]
#![allow(incomplete_features)]
#![feature(min_generic_const_args)]

extern crate alloc;

use enum_table::Enumable;

mod nvs;
pub use crate::nvs::*;

mod error;
pub use crate::error::*;

mod data;
// pub use crate::data::*;

mod key_map;
mod state;
mod linked_list;
mod cache;

pub trait NvsKey: Enumable + PartialEq
{
    #[type_const]
    const LEN: usize;
    
    fn from_key_value(value: u16) -> Self
    {
        return Self::VARIANTS[value as usize];
    }
    fn get_key_value(&self) -> u16
    {
        return self.variant_index() as u16;
    }
}

pub trait NvsConstants
{
    /// Max number of pages used by the map
    const MAPPING_MAX_RANGE: u8;
    const MAP_PRE_PADDING: u8;
    const STATE_PAGES: u8;
    /// From the first page of the map (should be at least `MAPPING_MAX_RANGE`)
    const MAP_POST_PADDING: u8;
    /// The number of copies of some data needed to fit into a new page before it is
    /// worth rewriting the entire page instead of moving to a new page
    // const REWRITE_COPY_SIZE_MULTIPLIER: u8 = 2;
    
    /// The total number of available pages
    const TOTAL_PAGES: u32;
    
    #[type_const]
    const PAGE_SIZE: u32;
    #[type_const]
    const WRITE_SIZE: usize;
    #[type_const]
    const READ_SIZE: usize;
    
    /// determines whether or not it is worth rewriting the entire page to fit in some data
    /// WARNING: this function needs to return `false` if the data cannot fit
    fn should_rewrite_page(remaining_space: u32, required_space: u32) -> bool
    {
        return remaining_space >= required_space * 2;
    }
}

macro_rules! round_up {
    ($value:expr, $align:expr) => {
        (($value + $align - 1) / $align) * $align
    };
}
pub(crate) use round_up;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Padding<V, const N: usize>(V, [u8; N]);
unsafe impl<V, const N: usize> bytemuck::Zeroable for Padding<V, N> {}
unsafe impl<V: bytemuck::Pod, const N: usize> bytemuck::Pod for Padding<V, N> {}
impl<V: bytemuck::Pod, const N: usize> Padding<V, N>
{
    pub fn as_bytes<'a>(&'a self, true_size: usize) -> &'a [u8]
    {
        return &bytemuck::bytes_of(self)[..true_size];
    }
    pub fn as_bytes_mut<'a>(&'a mut self, true_size: usize) -> &'a mut [u8]
    {
        return &mut bytemuck::bytes_of_mut(self)[..true_size];
    }
}
