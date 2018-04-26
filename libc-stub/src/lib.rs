//! This crate is a hack and should not exist!
//!
//! Ok with that out of the way let's see what's going on here. The libsodium
//! library is compiled with a C compiler (clang) and sort of expects to link
//! against musl. In reality we compiled it in such a way that is didn't find
//! most of its functionality (aka optional symbols are all deduced to not
//! exist). We won't link to musl as musl has more of a runtime than we'd like.
//!
//! Despite this the libsodium library will still reference some global symbols
//! it expects from the C library. Things like `abort` or `__errno_location`.
//! Again, these are all defined by musl itself, but for now we're not linking
//! that in. The MUSL library currently assumes a generic `__syscall` abi which
//! is a bit unfortunate, we don't want to deal with that at all! In any case..
//!
//! So to actually get these symbols resolved so we can instantiate the module
//! at runtime the definition has to live somewhere. Currently this opts to have
//! that definition live in Rust in this crate.
//!
//! This isn't great and is a massive pain point around integration with C right
//! now. It's not clear how these symbols are to be defined on the
//! `wasm32-unknown-unknown-wasm` target in C. I at least haven't seen a lot of
//! great examples of how to use that target in C...
//!
//! More comments on each function below!

// Nightly features needed to implement this crate
#![feature(allocator_api)]

// Nightly features required by `wasm_bindgen` currently
#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

#![allow(private_no_mangle_fns)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

use std::ffi::CStr;
use std::heap::{GlobalAlloc, Global, Layout};
use std::iter;
use std::mem;
use std::slice;

// This is used to access `errno` and is typically thread-local, but only one
// thread on wasm so we can make it a global
#[no_mangle]
pub unsafe extern fn __errno_location() -> *mut i32 {
    static mut ERRNO: i32 = 0;
    &mut ERRNO
}

// This is, well, `abort`. It can't return.
#[no_mangle]
pub unsafe extern fn abort() -> ! {
    wasm_bindgen::throw("abort");
}

// Shim called by the `assert` macro in C, we just send it off to `throw` like
// `abort` above
#[no_mangle]
pub unsafe extern fn __assert_fail(
    msg: *const i8,
    _file: *const i8,
    _line: i32,
    _func: *const i8,
) -> ! {
    let s = CStr::from_ptr(msg as *const _);
    wasm_bindgen::throw(s.to_str().unwrap_or("assert in C tripped"))
}

// Good ol' malloc and free. Looks like libsodium will do some memory
// allocation. That's handled here by routing to Rust's global memory allocator.
#[no_mangle]
pub unsafe extern fn malloc(a: usize) -> *mut u8 {
    let size = a + mem::size_of::<usize>();
    let layout = match Layout::from_size_align(size, mem::align_of::<usize>()) {
        Ok(n) => n,
        Err(_) => return 0 as *mut u8,
    };
    let ptr = Global.alloc(layout) as *mut usize;
    if ptr.is_null() {
        return ptr as *mut u8
    }
    *ptr.offset(0) = size;
    ptr.offset(1) as *mut u8
}

#[no_mangle]
pub unsafe extern fn free(ptr: *mut u8) {
    let ptr = (ptr as *mut usize).offset(-1);
    let size = *ptr.offset(0);
    let align = mem::size_of::<usize>();
    let layout = Layout::from_size_align_unchecked(size, align);
    Global.dealloc(ptr as *mut _, layout);
}

// Ok this is where functions get weird.
//
// Ideally we're defining as few functions as possible here, somehow coercing
// libsodium to believe it's in a very limited environment that doesn't have
// things like mprotect. We did a mostly good job of that but libsodium's random
// support is currently unconditionally included and the absolute fallback
// implementation is a dance of open/close/fstat/etc.
//
// These functions all exist to basically *only* support libsodium's
// requirements for a random number generator through `read`. These are not
// correct or valid implementations in general and probably aren't even correct
// for libsodium. This is the biggest hack of all!

#[no_mangle]
pub unsafe extern fn open(_a: i32, _b: i32) -> i32 {
    // TODO: check that the filename is `/dev/random` and only return an fd for
    // that, but this is called with a varargs ABI so difficult to check...
    3
}

// The current minimal subset needed to define `st_mode` to get past checks in
// libsodium.
#[allow(non_camel_case_types)]
pub struct stat {
    pub st_dev: i64,
    pub __std_dev_padding: u32,
    pub __st_ino_truncated: u32,
    pub st_mode: u32,
}

#[no_mangle]
pub unsafe extern fn fstat(fd: i32, s: *mut stat) -> i32 {
    if fd == 3 {
        (*s).st_mode = 0o20000; // make ST_ISCHR pass in libsodium
        0
    } else {
        -1
    }
}

#[no_mangle]
pub unsafe extern fn close(fd: i32) -> i32 {
    if fd == 3 { 0 } else { -1 }
}

#[no_mangle]
pub unsafe extern fn fcntl(_: i32, _: i32, _: i32) -> i32 {
    0
}

#[wasm_bindgen]
extern {
    #[wasm_bindgen(js_namespace = Math)]
    fn random() -> f64;
}

#[no_mangle]
pub unsafe extern fn read(fd: i32, bytes: *mut u8, amt: i32) -> i32 {
    if fd != 3 {
        return -1
    }

    // TODO: this is a terrible random number generator for a lot of reasons
    let rand = iter::repeat(())
        .map(|()| random())
        .flat_map(|f| {
            let bits = f.to_bits();
            (0..8).map(move |i| (bits >> (i * 8)) as u8)
        });
    let bytes = slice::from_raw_parts_mut(bytes, amt as usize);
    for (slot, val) in bytes.iter_mut().zip(rand) {
        *slot = val;
    }
    amt
}
