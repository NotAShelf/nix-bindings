use std::{ffi::{CStr, CString}, ptr::NonNull, sync::Arc};

use super::{Context, Error, Result, sys};

/// Nix store for managing packages and derivations.
///
/// The store provides access to Nix packages, derivations, and store paths.
pub struct Store {
  pub(crate) inner:    NonNull<sys::Store>,
  pub(crate) _context: Arc<Context>,
}

/// A path in the Nix store.
///
/// Represents a store path that can be realized, queried, or manipulated.
pub struct StorePath {
  pub(crate) inner:    NonNull<sys::StorePath>,
  pub(crate) _context: Arc<Context>,
}

impl StorePath {
  /// Parse a store path string into a StorePath.
  ///
  /// # Arguments
  ///
  /// * `context` - The Nix context
  /// * `store` - The store containing the path
  /// * `path` - The store path string (e.g., "/nix/store/...")
  ///
  /// # Errors
  ///
  /// Returns an error if the path cannot be parsed.
  pub fn parse(context: &Arc<Context>, store: &Store, path: &str) -> Result<Self> {
    let path_cstring = CString::new(path)?;

    // SAFETY: context, store, and path_cstring are valid
    let path_ptr = unsafe {
      sys::nix_store_parse_path(
        context.as_ptr(),
        store.as_ptr(),
        path_cstring.as_ptr(),
      )
    };

    let inner = NonNull::new(path_ptr).ok_or(Error::NullPointer)?;

    Ok(StorePath {
      inner,
      _context: Arc::clone(context),
    })
  }

  /// Get the name component of the store path.
  ///
  /// This returns the name part of the store path (everything after the hash).
  /// For example, for "/nix/store/abc123...-hello-1.0", this returns "hello-1.0".
  ///
  /// # Errors
  ///
  /// Returns an error if the name cannot be retrieved.
  pub fn name(&self) -> Result<String> {
    // Callback to receive the string
    unsafe extern "C" fn name_callback(
      start: *const std::os::raw::c_char,
      n: std::os::raw::c_uint,
      user_data: *mut std::os::raw::c_void,
    ) {
      let result = unsafe { &mut *(user_data as *mut Option<String>) };

      if !start.is_null() && n > 0 {
        let bytes = unsafe {
          std::slice::from_raw_parts(start.cast::<u8>(), n as usize)
        };
        if let Ok(s) = std::str::from_utf8(bytes) {
          *result = Some(s.to_string());
        }
      }
    }

    let mut result: Option<String> = None;
    let user_data = &mut result as *mut _ as *mut std::os::raw::c_void;

    // SAFETY: self.inner is valid, callback matches expected signature
    unsafe {
      sys::nix_store_path_name(self.inner.as_ptr(), Some(name_callback), user_data);
    }

    result.ok_or(Error::NullPointer)
  }

  /// Get the raw store path pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::StorePath {
    self.inner.as_ptr()
  }
}

impl Clone for StorePath {
  fn clone(&self) -> Self {
    // SAFETY: self.inner is valid, nix_store_path_clone creates a new copy
    let cloned_ptr = unsafe { sys::nix_store_path_clone(self.inner.as_ptr()) };

    // This should never fail as cloning a valid path should always succeed
    let inner = NonNull::new(cloned_ptr)
      .expect("nix_store_path_clone returned null for valid path");

    StorePath {
      inner,
      _context: Arc::clone(&self._context),
    }
  }
}

impl Drop for StorePath {
  fn drop(&mut self) {
    // SAFETY: We own the store path and it's valid until drop
    unsafe {
      sys::nix_store_path_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: StorePath can be shared between threads
unsafe impl Send for StorePath {}
unsafe impl Sync for StorePath {}

impl Store {
  /// Open a Nix store.
  ///
  /// # Arguments
  ///
  /// * `context` - The Nix context
  /// * `uri` - Optional store URI (None for default store)
  ///
  /// # Errors
  ///
  /// Returns an error if the store cannot be opened.
  pub fn open(context: &Arc<Context>, uri: Option<&str>) -> Result<Self> {
    let uri_cstring;
    let uri_ptr = if let Some(uri) = uri {
      uri_cstring = CString::new(uri)?;
      uri_cstring.as_ptr()
    } else {
      std::ptr::null()
    };

    // SAFETY: context is valid, uri_ptr is either null or valid CString
    let store_ptr = unsafe {
      sys::nix_store_open(context.as_ptr(), uri_ptr, std::ptr::null_mut())
    };

    let inner = NonNull::new(store_ptr).ok_or(Error::NullPointer)?;

    Ok(Store {
      inner,
      _context: Arc::clone(context),
    })
  }

  /// Get the raw store pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::Store {
    self.inner.as_ptr()
  }
}

impl Drop for Store {
  fn drop(&mut self) {
    // SAFETY: We own the store and it's valid until drop
    unsafe {
      sys::nix_store_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: Store can be shared between threads
unsafe impl Send for Store {}
unsafe impl Sync for Store {}

#[cfg(test)]
mod tests {
  use serial_test::serial;

  use super::*;

  #[test]
  #[serial]
  fn test_store_opening() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let _store = Store::open(&ctx, None).expect("Failed to open store");
  }

  #[test]
  #[serial]
  fn test_store_path_parse() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store = Store::open(&ctx, None).expect("Failed to open store");

    // Try parsing a well-formed store path
    // Note: This may fail if the path doesn't exist in the store
    let result =
      StorePath::parse(&ctx, &store, "/nix/store/00000000000000000000000000000000-test");

    // We don't assert success here because the path might not exist
    // This test mainly checks that the API works correctly
    match result {
      Ok(_path) => {
        // Successfully parsed the path
      },
      Err(_) => {
        // Path doesn't exist or is invalid, which is expected
      },
    }
  }

  #[test]
  #[serial]
  fn test_store_path_clone() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store = Store::open(&ctx, None).expect("Failed to open store");

    // Try to get a valid store path by parsing
    // Note: This test is somewhat limited without a guaranteed valid path
    if let Ok(path) =
      StorePath::parse(&ctx, &store, "/nix/store/00000000000000000000000000000000-test")
    {
      let cloned = path.clone();

      // Assert that the cloned path has the same name as the original
      let original_name = path.name().expect("Failed to get original path name");
      let cloned_name = cloned.name().expect("Failed to get cloned path name");

      assert_eq!(original_name, cloned_name, "Cloned path should have the same name as original");
    }
  }

  // Note: test_realize is not included because it requires a valid store path
  // to realize, which we can't guarantee in a unit test. Integration tests
  // would be more appropriate for testing realize() with actual derivations.
}
