#![allow(warnings, clippy::all)]

//! # nix-bindings-sys
//!
//! Raw, unsafe FFI bindings to the Nix C API.
//!
//! ## Safety
//!
//! These bindings are generated automatically and map directly to the C API.
//! They are unsafe to use directly. Prefer using the high-level safe API in the
//! parent crate unless you know what you're doing.
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

unsafe extern "C" {
  #[cfg(feature = "shim")]
  pub fn nix_store_add_text_to_store(
    context: *mut nix_c_context,
    store: *mut Store,
    name: *const std::os::raw::c_char,
    text: *const std::os::raw::c_char,
    text_len: std::os::raw::c_uint,
    out_path: *mut *mut StorePath,
  ) -> nix_err;
}
