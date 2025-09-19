#![deny(unsafe_code)]
#![warn(missing_docs)]
//! Rust bindings for the Nix build tool.
//!
//! This crate provides two types of bindings for your convenience:
//!
//! 1. Low-level bindings for the C API generated using Bindgen
//! 2. High-level, safe bindings by enforcing safety in the C API
//!
//! The goal is to provide ergonomic and idiomatic Rust APIs for interacting
//! with Nix using its C API. Raw, unsafe FFI bindings are available in the
//! `sys` module but their usage is discouraged.

/// Raw, unsafe FFI bindings to the Nix C API.
///
/// # Warning
///
/// This module exposes the low-level, unsafe C bindings. Prefer using the safe,
/// high-level APIs provided by this crate. Use at your own risk.
#[doc(hidden)]
pub mod sys {
  pub use nix_bindings_sys::*;
}
