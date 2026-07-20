# Wear Levelling Rust

A library that provides wear leveling, key value storage with no_std.
It is desgined for simple hardware like microcontrollers and intended for my own projects with the esp32s3 chip.

Currently it uses the unstable rust feature `generic_const_exprs`. I may change this to using `min_generic_const_args` at some point
as that seems more likely to come to stable release.