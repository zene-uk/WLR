# Wear Levelling Rust

A library that provides wear leveling, key value storage with no_std.
It is desgined for simple hardware like microcontrollers and intended for my own projects with the esp32s3 chip.

Currently it uses the unstable rust feature `min_generic_const_args`. It does not use this feature's intended new syntax yet,
but hopefully it comes to stable release soon.