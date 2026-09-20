#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(random)]

mod common;

use std::{collections::HashMap, ops::RangeFull, println, random};

use wlr::{Nvs, NvsKey};
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
    
    for _ in 0..10240
    {
        let i = random::random::<usize>(RangeFull) % 128;
        let result = nvs.write_key_values(random::random::<u16>(RangeFull).into(), &data[i]);
        assert!(result.is_ok());
    }
}

#[test]
fn write_test_2()
{
    let data: Box<_> = (0..128).map(|_| unsafe {
        Box::<[u8]>::new_uninit_slice(random::random::<usize>(RangeFull) % 128 + 1).assume_init()
    }).collect();
    
    let nor_flash = NorFlashEm::<4096>::new(1024);
    // let nor_flash = DebugFlash::new(nor_flash);
    let nvs = Nvs::<Key<1024>, _, Constants, 1024>::new(nor_flash);
    
    let mut hash = HashMap::new();
    
    assert!(nvs.is_ok());
    let mut nvs = nvs.unwrap();
    
    for _ in 0..10240
    {
        let i = random::random::<usize>(RangeFull) % 128;
        let k = random::random::<u16>(RangeFull).into();
        let d = &data[i];
        let result = nvs.write_key_values(k, d);
        assert!(result.is_ok());
        hash.insert(k, d);
    }
    
    for (k, v) in hash
    {
        let mut tmp: Box<[u8]> = unsafe { Box::new_uninit_slice(v.len()).assume_init() };
        let result = nvs.read_key_values(k, &mut tmp);
        assert!(result.is_ok());
        
        assert_eq!(v, &tmp);
        println!("{}", k.get_key_value());
    }
}