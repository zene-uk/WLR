#![no_std]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

extern crate alloc;

mod nvs;
use enum_table::Enumable;

pub use crate::nvs::*;

pub mod data;
// pub use crate::data::*;

mod key_map;
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
    const MAPPING_RANGE: u8;
    const MAP_PRE_PADDING: u8;
    const STATE_COPIES: u8;
    const MAP_POST_PADDING: u8;
}
