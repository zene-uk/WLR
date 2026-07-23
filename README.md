# Wear Levelling Rust

A library that provides wear levelled, key-value storage with `no_std`.
It is desgined for simple hardware like microcontrollers and intended for my own projects with the esp32s3 chip.

~~Currently it uses the unstable rust feature `min_generic_const_args`. It does not use this feature's intended new syntax yet,
but hopefully it comes to stable release soon.~~

`min_generic_const_args` was not happy when I tried to compile so I switched back to `generic_const_expr`.

Currently it also requires regularly heap allocating data. I may try and mitigate this in the future, but for now my uses don't need super stable
memory for extremely long periods.