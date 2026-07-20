#![no_std]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

extern crate alloc;

mod nvs;
use enum_table::Enumable;

pub use crate::nvs::*;

mod data;
// pub use crate::data::*;

mod key_map;
mod state;
mod linked_list;

pub trait True {}
pub struct CheckConst<const B: bool>;
impl True for CheckConst<true> {}

pub trait NvsKey: Enumable
{
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
    const MAPPING_MAX_RANGE: u8;
    const MAP_PRE_PADDING: u8;
    const STATE_PAGES: u8;
    /// From the first page of the map (should be at least `MAPPING_MAX_RANGE`)
    const MAP_POST_PADDING: u8;
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
