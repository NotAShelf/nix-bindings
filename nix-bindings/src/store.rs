use std::{ffi::CString, ptr::NonNull, sync::Arc};
use super::{sys, Context, Result, Error};

/// Nix store for managing packages and derivations.
///
/// The store provides access to Nix packages, derivations, and store paths.
pub struct Store {
  pub(crate) inner:    NonNull<sys::Store>,
  pub(crate) _context: Arc<Context>,
}

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
  use super::*;
  use serial_test::serial;

  #[test]
  #[serial]
  fn test_store_opening() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let _store = Store::open(&ctx, None).expect("Failed to open store");
  }
}

