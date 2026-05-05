//! Nix flake settings.
//!
//! This module provides [`FlakeSettings`], which configures flake support for
//! the Nix evaluator. Pass a [`FlakeSettings`] to
//! [`EvalStateBuilder::with_flake_settings`](crate::EvalStateBuilder::with_flake_settings)
//! to enable `builtins.getFlake` and related functionality.

use std::{ptr::NonNull, sync::Arc};

use crate::{Context, Error, Result, sys};

/// Configuration for the Nix flake subsystem.
///
/// This enables flake evaluation features in the Nix evaluator (such as
/// `builtins.getFlake`). Obtain a `FlakeSettings` and pass it to
/// [`EvalStateBuilder::with_flake_settings`](crate::EvalStateBuilder::with_flake_settings)
/// before building the [`EvalState`](crate::EvalState).
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
///
/// use nix_bindings::{Context, EvalStateBuilder, Store, flake::FlakeSettings};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = Arc::new(Context::new()?);
/// let store = Arc::new(Store::open(&ctx, None)?);
///
/// let flake_settings = FlakeSettings::new(&ctx)?;
///
/// let state = EvalStateBuilder::new(&store)?
///   .with_flake_settings(&flake_settings)?
///   .build()?;
/// # Ok(())
/// # }
/// ```
pub struct FlakeSettings {
  inner:    NonNull<sys::nix_flake_settings>,
  _context: Arc<Context>,
}

impl FlakeSettings {
  /// Create a new set of flake settings with default values.
  ///
  /// # Errors
  ///
  /// Returns an error if the underlying allocation fails.
  pub fn new(context: &Arc<Context>) -> Result<Self> {
    // SAFETY: context is valid
    let ptr = unsafe { sys::nix_flake_settings_new(context.as_ptr()) };

    let inner = NonNull::new(ptr).ok_or(Error::NullPointer)?;

    Ok(FlakeSettings {
      inner,
      _context: Arc::clone(context),
    })
  }

  /// Get the raw flake settings pointer.
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::nix_flake_settings {
    self.inner.as_ptr()
  }
}

impl Drop for FlakeSettings {
  fn drop(&mut self) {
    // SAFETY: We own the settings and they are valid until drop
    unsafe {
      sys::nix_flake_settings_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: FlakeSettings can be shared between threads
unsafe impl Send for FlakeSettings {}
unsafe impl Sync for FlakeSettings {}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use serial_test::serial;

  use super::*;

  #[test]
  #[serial]
  fn test_flake_settings_new() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let _settings =
      FlakeSettings::new(&ctx).expect("Failed to create flake settings");
  }

  #[test]
  #[serial]
  fn test_flake_settings_with_eval_state() {
    use crate::{EvalStateBuilder, Store};
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let settings =
      FlakeSettings::new(&ctx).expect("Failed to create flake settings");

    let _state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .with_flake_settings(&settings)
      .expect("Failed to apply flake settings")
      .build()
      .expect("Failed to build state");
  }
}
