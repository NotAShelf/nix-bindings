//! Shared value-access trait for crate-level [`Value`](crate::Value) and the
//! callback-scoped wrappers in [`primop`](crate::primop).
//!
//! Bring [`NixValueOps`] into scope to use the read accessors generically:
//!
//! ```ignore
//! use nix_bindings::NixValueOps;
//!
//! fn sum<V: NixValueOps>(values: &[V]) -> nix_bindings::Result<i64> {
//!   values.iter().map(|v| v.as_int()).sum()
//! }
//! ```

use std::ffi::CStr;

use crate::{Error, Result, ValueType, check_err, sys};

pub(crate) mod sealed {
  use super::sys;

  pub trait NixValueRaw {
    fn raw_ctx(&self) -> *mut sys::nix_c_context;
    fn raw_state(&self) -> *mut sys::EvalState;
    fn raw_inner(&self) -> *mut sys::nix_value;
  }
}

// Re-exported under the module namespace so other crate modules can
// `impl crate::value_ops::NixValueRaw for Foo` to opt their type into
// [`NixValueOps`]. The trait stays sealed by the private `sealed`
// module: downstream crates cannot name it.
pub(crate) use sealed::NixValueRaw;

/// Read access to a Nix value.
///
/// Implemented by [`Value`](crate::Value),
/// [`primop::PrimOpArg`](crate::primop::PrimOpArg), and
/// [`primop::PrimOpValue`](crate::primop::PrimOpValue). All accessors force
/// the value first, mirroring `nix-instantiate`-style semantics: a lazy
/// attribute that resolves to an int is reported as an int, not a thunk.
pub trait NixValueOps: sealed::NixValueRaw {
  /// Return the [`ValueType`] of this value.
  ///
  /// Does **not** force; an unforced thunk reports as [`ValueType::Thunk`].
  /// Use [`force`](Self::force) (or any `as_*` accessor) to resolve first.
  fn value_type(&self) -> ValueType {
    // SAFETY: ctx and inner are valid for the wrapper's lifetime.
    let c_type = unsafe { sys::nix_get_type(self.raw_ctx(), self.raw_inner()) };
    ValueType::from_c(c_type)
  }

  /// Force evaluation (resolves thunks).
  ///
  /// # Errors
  ///
  /// Returns an error if evaluation fails.
  fn force(&self) -> Result<()> {
    // SAFETY: ctx, state, and inner are valid for the wrapper's lifetime.
    unsafe {
      check_err(
        self.raw_ctx(),
        sys::nix_value_force(
          self.raw_ctx(),
          self.raw_state(),
          self.raw_inner(),
        ),
      )
    }
  }

  /// Extract as an integer.  Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not an
  /// integer.
  fn as_int(&self) -> Result<i64> {
    self.force()?;
    if self.value_type() != ValueType::Int {
      return Err(Error::InvalidType {
        expected: "int",
        actual:   self.value_type().to_string(),
      });
    }
    // SAFETY: type checked.
    Ok(unsafe { sys::nix_get_int(self.raw_ctx(), self.raw_inner()) })
  }

  /// Extract as a float.  Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// float.
  fn as_float(&self) -> Result<f64> {
    self.force()?;
    if self.value_type() != ValueType::Float {
      return Err(Error::InvalidType {
        expected: "float",
        actual:   self.value_type().to_string(),
      });
    }
    // SAFETY: type checked.
    Ok(unsafe { sys::nix_get_float(self.raw_ctx(), self.raw_inner()) })
  }

  /// Extract as a boolean.  Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// boolean.
  fn as_bool(&self) -> Result<bool> {
    self.force()?;
    if self.value_type() != ValueType::Bool {
      return Err(Error::InvalidType {
        expected: "bool",
        actual:   self.value_type().to_string(),
      });
    }
    // SAFETY: type checked.
    Ok(unsafe { sys::nix_get_bool(self.raw_ctx(), self.raw_inner()) })
  }

  /// Extract as a UTF-8 string, realising any string context.  Forces
  /// the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails, the resolved value is not a
  /// string, or the string contains invalid UTF-8.
  fn as_string(&self) -> Result<String> {
    self.force()?;
    if self.value_type() != ValueType::String {
      return Err(Error::InvalidType {
        expected: "string",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: type checked.
    let realised_str = unsafe {
      sys::nix_string_realise(
        self.raw_ctx(),
        self.raw_state(),
        self.raw_inner(),
        false,
      )
    };

    if realised_str.is_null() {
      return Err(Error::NullPointer);
    }

    let buffer_start =
      unsafe { sys::nix_realised_string_get_buffer_start(realised_str) };
    let buffer_size =
      unsafe { sys::nix_realised_string_get_buffer_size(realised_str) };

    if buffer_start.is_null() {
      unsafe { sys::nix_realised_string_free(realised_str) };
      return Err(Error::NullPointer);
    }

    let bytes = unsafe {
      std::slice::from_raw_parts(buffer_start.cast::<u8>(), buffer_size)
    };
    let s = std::str::from_utf8(bytes)
      .map_err(|_| Error::Unknown("Invalid UTF-8 in string".into()))?
      .to_owned();

    unsafe { sys::nix_realised_string_free(realised_str) };
    Ok(s)
  }

  /// Extract as a filesystem-path string.  Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails, the resolved value is not a
  /// path, or the path contains invalid UTF-8.
  fn as_path(&self) -> Result<String> {
    self.force()?;
    if self.value_type() != ValueType::Path {
      return Err(Error::InvalidType {
        expected: "path",
        actual:   self.value_type().to_string(),
      });
    }
    // SAFETY: type checked; pointer outlives the copy.
    let raw =
      unsafe { sys::nix_get_path_string(self.raw_ctx(), self.raw_inner()) };
    if raw.is_null() {
      return Err(Error::NullPointer);
    }
    let cstr = unsafe { CStr::from_ptr(raw) };
    cstr
      .to_str()
      .map(str::to_owned)
      .map_err(|_| Error::Unknown("Invalid UTF-8 in path".into()))
  }
}

impl<T: sealed::NixValueRaw> NixValueOps for T {}
