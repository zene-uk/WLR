#![no_std]

mod nvs;
pub use crate::nvs::*;

pub mod data;
// pub use crate::data::*;

pub trait NvsConstants
{
    const MAPPING_RANGE: u8;
    const MAP_PRE_PADDING: u8;
    const STATE_COPIES: u8;
    const MAP_POST_PADDING: u8;
}