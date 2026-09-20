# Wear Levelling Rust

A library that provides wear levelled, key-value storage with `no_std`.
It is desgined for simple hardware like microcontrollers and intended for my own projects with the esp32s3 chip.

~~Currently it uses the unstable rust feature `min_generic_const_args`. It does not use this feature's intended new syntax yet,
but hopefully it comes to stable release soon.~~

`min_generic_const_args` was not happy when I tried to compile so I switched back to `generic_const_expr`.

Currently it also requires regularly heap allocating data. I may try and mitigate this in the future, but for now my uses don't need indefinate stable memory.

## Design

### Start Pages
The first few pages are allocated to the map reference. This is the page index (from the very start) of the start of the mapping region. These pages have leading 0s untill the page index, and then trailing 1s. This is so every time the index changes, it only moves along 1-4 bytes. e.g. an index of 47 could look like:

|||||
|--|--|--|--|
|`0x00`|`0x00`|`0x00`|`0x00`|
|`0x2F`|`0xFF`|`0xFF`|`0xFF`|

### Data Pages
The main bulk of the pages are allocated to the actual data, but data blocks are limited to page size, but entries are allowed to overflow into the next page for optimal packing. Data is stored as is without any extra metadata.

### Mappings Pages
Then there are the mapping pages. These store 8 byte entries, 2 bytes for the key, 2 bytes for the data size and 4 bytes for the data address: `kkkkkkkkkkkkkkkkssssssssssssssssaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`.

Like the start pages, if the key and size bytes are all 0s, then the entry has been written to but is no longer used. If they are all 1s, then the entire entry is all 1s so it is unused and ready to write to. All entries following the first available one are also available. If the key-size bytes have a value of `0xFFFF0000`, then the address points to the next data write location. e.g. a mapping page could look like:

|||||||||
|--|--|--|--|--|--|--|--|
|`0x00`|`0x00`|`0x00`|`0x00`|`0x23`|`0x9B`|`0xAF`|`0xC3`|
|`0x00`|`0x00`|`0x00`|`0x00`|`0x23`|`0x9D`|`0x00`|`0x10`|
|`0x11`|`0x11`|`0x00`|`0x40`|`0x23`|`0x9E`|`0x00`|`0x00`|
|`0xFF`|`0xFF`|`0x00`|`0x00`|`0x23`|`0x9E`|`0x00`|`0x40`|

The mapping pages keep using the next available entry for each change until all of the space is used up. If the range of the mapping pages becomes too long, the back page entries are moved to the front (this process could keep repeating for a while if there are a lot of gaps without the space to keep them). Used data pages encountered whilst doing this process are also moved (obviously along with their entries).

There can also be padding pages around the mapping region (that the next data address jumps over) so that they do write to those pages too often.

This rolling design is what turns this into a sort of static wear leveling. As all pages are constantly cycled through by the mapping region.