#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
//! Raw, unsafe FFI bindings to the Nix C API.
//!
//! # Safety
//! These bindings are generated automatically and map directly to the C API.
//! They are unsafe to use directly. Prefer using the high-level safe API in the
//! parent crate unless you know what you're doing.

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
