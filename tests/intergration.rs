#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod common;

use wlr::Nvs;
use crate::common::{Constants, Key, NorFlashEm};

#[test]
fn init_test()
{
    let nor_flash = NorFlashEm::<4096>::new(1024);
    let nvs = Nvs::<Key<1024>, _, Constants, 1024>::new(nor_flash);
    
    assert!(nvs.is_ok());
}