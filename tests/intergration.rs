#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(random)]

mod common;

use std::{ops::RangeFull, println, random};

use wlr::Nvs;
use crate::common::{Constants, DebugFlash, Key, NorFlashEm};

#[test]
fn init_test()
{
    let nor_flash = NorFlashEm::<4096>::new(1024);
    let nvs = Nvs::<Key<1024>, _, Constants, 1024>::new(nor_flash);
    
    assert!(nvs.is_ok());
}

#[test]
fn write_test()
{
    let data: Box<_> = (0..128).map(|_| unsafe {
        Box::<[u8]>::new_uninit_slice(random::random::<usize>(RangeFull) % 128 + 1).assume_init()
    }).collect();
    
    let nor_flash = NorFlashEm::<4096>::new(1024);
    let print_flash = DebugFlash::new(nor_flash);
    let nvs = Nvs::<Key<1024>, _, Constants, 1024>::new(print_flash);
    
    assert!(nvs.is_ok());
    let mut nvs = nvs.unwrap();
    
    for j in 0..10240
    {
        let i = random::random::<usize>(RangeFull) % 128;
        let result = nvs.write_key_values(random::random::<u16>(RangeFull).into(), &data[i]);
        assert!(result.is_ok());
        println!("{j}");
    }
}