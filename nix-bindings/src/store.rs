use std::{
  ffi::{CStr, CString},
  ptr::NonNull,
  sync::Arc,
};

use super::{Context, Error, Result, check_err, string_from_callback, sys};

/// Nix store for managing packages and derivations.
///
/// The store provides access to Nix packages, derivations, and store paths.
pub struct Store {
  pub(crate) inner:    NonNull<sys::Store>,
  pub(crate) _context: Arc<Context>,
}

/// A path in the Nix store.
///
/// Represents a store path that can be realized or queried.
pub struct StorePath {
  pub(crate) inner:    NonNull<sys::StorePath>,
  pub(crate) _context: Arc<Context>,
}

/// A Nix derivation loaded from its JSON representation.
///
/// Derivations are the build recipes used by the Nix store. They describe
/// how to produce a store path from inputs. Use [`Derivation::from_json`]
/// to construct one and [`Derivation::add_to_store`] to register it.
pub struct Derivation {
  inner:    *mut sys::nix_derivation,
  _context: Arc<Context>,
}

impl StorePath {
  /// Parse a store path string into a `StorePath`.
  ///
  /// # Arguments
  ///
  /// * `context` - The Nix context
  /// * `store` - The store containing the path
  /// * `path` - The store path string (e.g., `"/nix/store/..."`)
  ///
  /// # Errors
  ///
  /// Returns an error if the path string is not a valid store path.
  pub fn parse(
    context: &Arc<Context>,
    store: &Store,
    path: &str,
  ) -> Result<Self> {
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
  /// Returns the name part of the store path (everything after the hash).
  /// For example, for `"/nix/store/abc123...-hello-1.0"` this returns
  /// `"hello-1.0"`.
  ///
  /// # Errors
  ///
  /// Returns an error if the name cannot be retrieved.
  pub fn name(&self) -> Result<String> {
    // SAFETY: self.inner is valid, callback matches expected signature
    let result = unsafe {
      string_from_callback(|cb, ud| {
        sys::nix_store_path_name(self.inner.as_ptr(), cb, ud);
      })
    };
    result.ok_or(Error::NullPointer)
  }

  /// Get the raw store path pointer.
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::StorePath {
    self.inner.as_ptr()
  }
}

impl Clone for StorePath {
  fn clone(&self) -> Self {
    // SAFETY: self.inner is valid
    let cloned_ptr = unsafe { sys::nix_store_path_clone(self.inner.as_ptr()) };

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
    // SAFETY: We own the store path and it is valid until drop
    unsafe {
      sys::nix_store_path_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: StorePath can be shared between threads
unsafe impl Send for StorePath {}
unsafe impl Sync for StorePath {}

impl Derivation {
  /// Parse a derivation from its JSON representation.
  ///
  /// # Arguments
  ///
  /// * `context` - The Nix context
  /// * `store` - The store to use
  /// * `json` - JSON string describing the derivation
  ///
  /// # Errors
  ///
  /// Returns an error if the JSON is not a valid derivation description.
  pub fn from_json(
    context: &Arc<Context>,
    store: &Store,
    json: &str,
  ) -> Result<Self> {
    let json_c = CString::new(json)?;

    // SAFETY: context, store, and json_c are valid
    let drv_ptr = unsafe {
      sys::nix_derivation_from_json(
        context.as_ptr(),
        store.as_ptr(),
        json_c.as_ptr(),
      )
    };

    if drv_ptr.is_null() {
      return Err(Error::NullPointer);
    }

    Ok(Derivation {
      inner:    drv_ptr,
      _context: Arc::clone(context),
    })
  }

  /// Add this derivation to the store and return its output store path.
  ///
  /// # Errors
  ///
  /// Returns an error if the derivation cannot be registered in the store.
  pub fn add_to_store(&mut self, store: &Store) -> Result<StorePath> {
    // SAFETY: context, store, and inner are valid
    let path_ptr = unsafe {
      sys::nix_add_derivation(
        self._context.as_ptr(),
        store.as_ptr(),
        self.inner,
      )
    };

    let inner = NonNull::new(path_ptr).ok_or(Error::NullPointer)?;

    Ok(StorePath {
      inner,
      _context: Arc::clone(&self._context),
    })
  }
}

impl Drop for Derivation {
  fn drop(&mut self) {
    // SAFETY: We own the derivation and it is valid until drop
    unsafe {
      sys::nix_derivation_free(self.inner);
    }
  }
}

// SAFETY: Derivation can be shared between threads
unsafe impl Send for Derivation {}
unsafe impl Sync for Derivation {}

impl Store {
  /// Open a Nix store.
  ///
  /// # Arguments
  ///
  /// * `context` - The Nix context
  /// * `uri` - Optional store URI (`None` for the default local store)
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

    // SAFETY: context is valid; uri_ptr is either null or a valid CString
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
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::Store {
    self.inner.as_ptr()
  }

  /// Realize a store path.
  ///
  /// Builds or downloads the store path and all its dependencies, making
  /// them available in the local store.
  ///
  /// # Returns
  ///
  /// A vector of `(output_name, store_path)` pairs for each realized output.
  ///
  /// # Errors
  ///
  /// Returns an error if the path cannot be realized.
  pub fn realize(&self, path: &StorePath) -> Result<Vec<(String, StorePath)>> {
    type Userdata = (Vec<(String, StorePath)>, Arc<Context>);

    unsafe extern "C" fn realize_callback(
      userdata: *mut std::os::raw::c_void,
      outname: *const std::os::raw::c_char,
      out: *const sys::StorePath,
    ) {
      let data = unsafe { &mut *(userdata as *mut Userdata) };
      let (outputs, context) = data;

      let name = if !outname.is_null() {
        unsafe { CStr::from_ptr(outname).to_string_lossy().into_owned() }
      } else {
        String::from("out")
      };

      if !out.is_null() {
        let cloned_path =
          unsafe { sys::nix_store_path_clone(out as *mut sys::StorePath) };
        if let Some(inner) = NonNull::new(cloned_path) {
          outputs.push((name, StorePath {
            inner,
            _context: Arc::clone(context),
          }));
        }
      }
    }

    let mut userdata: Userdata = (Vec::new(), Arc::clone(&self._context));
    let userdata_ptr =
      &mut userdata as *mut Userdata as *mut std::os::raw::c_void;

    let err = unsafe {
      sys::nix_store_realise(
        self._context.as_ptr(),
        self.inner.as_ptr(),
        path.as_ptr(),
        userdata_ptr,
        Some(realize_callback),
      )
    };

    check_err(unsafe { self._context.as_ptr() }, err)?;

    Ok(userdata.0)
  }

  /// Parse a store path string into a [`StorePath`].
  ///
  /// Convenience wrapper around [`StorePath::parse`].
  ///
  /// # Errors
  ///
  /// Returns an error if the path cannot be parsed.
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use std::sync::Arc;
  /// # use nix_bindings::{Context, Store};
  /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
  /// let ctx = Arc::new(Context::new()?);
  /// let store = Store::open(&ctx, None)?;
  /// let path = store.store_path("/nix/store/...")?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn store_path(&self, path: &str) -> Result<StorePath> {
    StorePath::parse(&self._context, self, path)
  }

  /// Check whether a store path is present and valid in the store.
  ///
  /// Returns `true` if the path exists in the store's database.
  #[must_use]
  pub fn is_valid_path(&self, path: &StorePath) -> bool {
    // SAFETY: context, store, and path are valid
    unsafe {
      sys::nix_store_is_valid_path(
        self._context.as_ptr(),
        self.inner.as_ptr(),
        path.inner.as_ptr(),
      )
    }
  }

  /// Resolve the real filesystem path for a store path.
  ///
  /// For content-addressed stores (e.g., a binary cache) this may differ
  /// from the store path itself.
  ///
  /// # Errors
  ///
  /// Returns an error if the path cannot be resolved.
  pub fn real_path(&self, path: &StorePath) -> Result<String> {
    // SAFETY: context, store, and path are valid; callback is safe
    let result = unsafe {
      string_from_callback(|cb, ud| {
        sys::nix_store_real_path(
          self._context.as_ptr(),
          self.inner.as_ptr(),
          path.inner.as_ptr(),
          cb,
          ud,
        );
      })
    };
    result.ok_or(Error::NullPointer)
  }

  /// Get the URI identifying this store (e.g., `"local"` or
  /// `"https://cache.nixos.org"`).
  ///
  /// # Errors
  ///
  /// Returns an error if the URI cannot be retrieved.
  pub fn uri(&self) -> Result<String> {
    // SAFETY: context and store are valid; callback is safe
    let result = unsafe {
      string_from_callback(|cb, ud| {
        sys::nix_store_get_uri(
          self._context.as_ptr(),
          self.inner.as_ptr(),
          cb,
          ud,
        );
      })
    };
    result.ok_or(Error::NullPointer)
  }

  /// Get the store directory (e.g., `"/nix/store"`).
  ///
  /// # Errors
  ///
  /// Returns an error if the directory cannot be retrieved.
  pub fn store_dir(&self) -> Result<String> {
    // SAFETY: context and store are valid; callback is safe
    let result = unsafe {
      string_from_callback(|cb, ud| {
        sys::nix_store_get_storedir(
          self._context.as_ptr(),
          self.inner.as_ptr(),
          cb,
          ud,
        );
      })
    };
    result.ok_or(Error::NullPointer)
  }

  /// Get the version string of the store daemon.
  ///
  /// # Errors
  ///
  /// Returns an error if the version cannot be retrieved.
  pub fn version(&self) -> Result<String> {
    // SAFETY: context and store are valid; callback is safe
    let result = unsafe {
      string_from_callback(|cb, ud| {
        sys::nix_store_get_version(
          self._context.as_ptr(),
          self.inner.as_ptr(),
          cb,
          ud,
        );
      })
    };
    result.ok_or(Error::NullPointer)
  }

  /// Copy the closure of `path` from `self` into `dst_store`.
  ///
  /// This copies the store path and all its transitive dependencies.
  ///
  /// # Errors
  ///
  /// Returns an error if the copy operation fails.
  pub fn copy_closure(
    &self,
    dst_store: &Store,
    path: &StorePath,
  ) -> Result<()> {
    // SAFETY: context, src store, dst store, and path are valid
    let err = unsafe {
      sys::nix_store_copy_closure(
        self._context.as_ptr(),
        self.inner.as_ptr(),
        dst_store.as_ptr(),
        path.inner.as_ptr(),
      )
    };
    check_err(unsafe { self._context.as_ptr() }, err)
  }
}

impl Drop for Store {
  fn drop(&mut self) {
    // SAFETY: We own the store and it is valid until drop
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
  use std::sync::Arc;

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

    // Well-formed path; may or may not exist in the local store
    let result = StorePath::parse(
      &ctx,
      &store,
      "/nix/store/00000000000000000000000000000000-test",
    );

    match result {
      Ok(_) | Err(_) => {
        // Either outcome is acceptable; we just verify the API does not
        // panic or invoke UB
      },
    }
  }

  #[test]
  #[serial]
  fn test_store_path_clone() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store = Store::open(&ctx, None).expect("Failed to open store");

    if let Ok(path) = StorePath::parse(
      &ctx,
      &store,
      "/nix/store/00000000000000000000000000000000-test",
    ) {
      let cloned = path.clone();
      let original_name = path.name().expect("Failed to get original name");
      let cloned_name = cloned.name().expect("Failed to get cloned name");
      assert_eq!(
        original_name, cloned_name,
        "Cloned path should have the same name"
      );
    }
  }

  #[test]
  #[serial]
  fn test_store_uri() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store = Store::open(&ctx, None).expect("Failed to open store");
    let uri = store.uri().expect("Failed to get store URI");
    assert!(!uri.is_empty(), "Store URI should not be empty");
  }

  #[test]
  #[serial]
  fn test_store_dir() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store = Store::open(&ctx, None).expect("Failed to open store");
    let dir = store.store_dir().expect("Failed to get store directory");
    assert!(!dir.is_empty(), "Store directory should not be empty");
  }

  #[test]
  #[serial]
  fn test_store_version() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store = Store::open(&ctx, None).expect("Failed to open store");
    let ver = store.version().expect("Failed to get store version");
    assert!(!ver.is_empty(), "Store version should not be empty");
  }

  #[test]
  #[serial]
  fn test_store_is_valid_path() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store = Store::open(&ctx, None).expect("Failed to open store");

    if let Ok(path) = StorePath::parse(
      &ctx,
      &store,
      "/nix/store/00000000000000000000000000000000-test",
    ) {
      // A random hash is almost certainly not valid
      let valid = store.is_valid_path(&path);
      assert!(!valid, "Random path should not be valid in the store");
    }
  }
}
