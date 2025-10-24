#![warn(missing_docs)]
//! High-level, safe Rust bindings for the Lix build tool.
//!
//! This crate provides ergonomic and idiomatic Rust APIs for interacting
//! with Lix using its C++ API via C wrappers.
use std::{
  ffi::CString,
  os::raw::{c_char, c_void},
  ptr,
};

#[cfg(test)] use serial_test::serial;

/// Result type for Lix operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for Lix operations.
#[derive(Debug)]
pub enum Error {
  /// Unknown error from Lix C API.
  Unknown(String),

  /// Null pointer error.
  NullPointer,

  /// String conversion error.
  StringConversion(std::ffi::NulError),
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Error::Unknown(msg) => write!(f, "Unknown error: {msg}"),
      Error::NullPointer => write!(f, "Null pointer error"),
      Error::StringConversion(e) => write!(f, "String conversion error: {e}"),
    }
  }
}

impl std::error::Error for Error {}

impl From<std::ffi::NulError> for Error {
  fn from(e: std::ffi::NulError) -> Self {
    Error::StringConversion(e)
  }
}

/// FFI bindings to Lix C wrapper.
extern "C" {
  fn lix_wrapper_init();
  fn lix_wrapper_open_store() -> *mut c_void;
  fn lix_wrapper_parse_store_path(
    store: *mut c_void,
    path: *const c_char,
  ) -> *mut c_void;
  fn lix_wrapper_build_path(store: *mut c_void, path: *mut c_void) -> i32;
  fn lix_wrapper_free_string(s: *const c_char);
  fn lix_wrapper_free_pointer(ptr: *mut c_void);
}

/// Lix store for managing packages and derivations.
///
/// The store provides access to Lix packages, derivations, and store paths.
pub struct Store {
  inner: *mut c_void,
}

impl Store {
  /// Open a Lix store.
  ///
  /// This initializes the Lix library and opens the default store.
  ///
  /// # Errors
  ///
  /// Returns an error if the store cannot be opened.
  pub fn open() -> Result<Self> {
    unsafe {
      lix_wrapper_init();
      let store_ptr = lix_wrapper_open_store();
      if store_ptr.is_null() {
        return Err(Error::NullPointer);
      }
      Ok(Store { inner: store_ptr })
    }
  }

  /// Parse a store path.
  ///
  /// # Arguments
  ///
  /// * `path` - The path to parse
  ///
  /// # Errors
  ///
  /// Returns an error if parsing fails.
  pub fn parse_store_path(&self, path: &str) -> Result<StorePath> {
    let path_c = CString::new(path)?;
    unsafe {
      let path_ptr = lix_wrapper_parse_store_path(self.inner, path_c.as_ptr());
      if path_ptr.is_null() {
        return Err(Error::NullPointer);
      }
      Ok(StorePath { inner: path_ptr })
    }
  }

  /// Build a derivation path.
  ///
  /// # Arguments
  ///
  /// * `path` - The store path to build
  ///
  /// # Errors
  ///
  /// Returns an error if building fails.
  pub fn build_path(&self, path: &StorePath) -> Result<()> {
    unsafe {
      let err = lix_wrapper_build_path(self.inner, path.inner);
      if err != 0 {
        return Err(Error::Unknown("Build failed".to_string()));
      }
      Ok(())
    }
  }
}

impl Drop for Store {
  fn drop(&mut self) {
    unsafe {
      lix_wrapper_free_pointer(self.inner);
    }
  }
}

/// A parsed store path.
pub struct StorePath {
  inner: *mut c_void,
}

impl Drop for StorePath {
  fn drop(&mut self) {
    unsafe {
      lix_wrapper_free_pointer(self.inner);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[serial]
  fn test_store_opening() {
    // Store should be dropped automatically
    let _store = Store::open().expect("Failed to open store");
  }

  #[test]
  #[serial]
  fn test_parse_store_path() {
    let store = Store::open().expect("Failed to open store");

    // Path should be dropped automatically
    let _path = store
      .parse_store_path("/nix/store/test")
      .expect("Failed to parse path");
  }
}
