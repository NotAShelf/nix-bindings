//! Error types and FFI error-handling helpers.
//!
//! Re-exported at the crate root as [`crate::Error`] and [`crate::Result`].

use std::{ffi::CStr, fmt};

use crate::sys;

/// Result type for Nix operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for Nix operations.
#[derive(Debug)]
pub enum Error {
  /// Unknown error from Nix C API.
  Unknown(String),

  /// Overflow error.
  Overflow,

  /// Key not found error.
  KeyNotFound(String),

  /// List index out of bounds.
  IndexOutOfBounds {
    /// The index that was requested.
    index:  usize,
    /// The actual length of the list.
    length: usize,
  },

  /// Nix evaluation error.
  EvalError(String),

  /// Invalid value type conversion.
  InvalidType {
    /// Expected type.
    expected: &'static str,
    /// Actual type.
    actual:   String,
  },
  /// Null pointer error.
  NullPointer,

  /// String conversion error.
  StringConversion(std::ffi::NulError),
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Error::Unknown(msg) => write!(f, "Unknown error: {msg}"),
      Error::Overflow => write!(f, "Overflow error"),
      Error::KeyNotFound(key) => write!(f, "Key not found: {key}"),
      Error::IndexOutOfBounds { index, length } => {
        write!(f, "Index out of bounds: index {index}, length {length}")
      },
      Error::EvalError(msg) => write!(f, "Evaluation error: {msg}"),
      Error::InvalidType { expected, actual } => {
        write!(f, "Invalid type: expected {expected}, got {actual}")
      },
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

/// Extract a string from a Nix context using a callback-based API.
///
/// Many Nix C API functions return strings via callbacks. This helper
/// makes that pattern ergonomic.
///
/// # Safety
///
/// `call` must invoke `callback` with a valid string pointer and length.
#[cfg(feature = "store")]
pub(crate) unsafe fn string_from_callback<F>(call: F) -> Option<String>
where
  F: FnOnce(sys::nix_get_string_callback, *mut std::os::raw::c_void),
{
  unsafe extern "C" fn collect(
    start: *const std::os::raw::c_char,
    n: std::os::raw::c_uint,
    user_data: *mut std::os::raw::c_void,
  ) {
    let result = unsafe { &mut *(user_data as *mut Option<String>) };
    if !start.is_null() {
      let bytes =
        unsafe { std::slice::from_raw_parts(start.cast::<u8>(), n as usize) };
      *result = std::str::from_utf8(bytes).ok().map(|s| s.to_owned());
    }
  }

  let mut result: Option<String> = None;
  let user_data = &mut result as *mut _ as *mut std::os::raw::c_void;
  call(Some(collect), user_data);
  result
}

/// Check a Nix error code and convert to `Result`, extracting the real
/// error message from the context.
#[cfg(feature = "store")]
pub(crate) fn check_err(
  ctx: *mut sys::nix_c_context,
  err: sys::nix_err,
) -> Result<()> {
  if err == sys::nix_err_NIX_OK {
    return Ok(());
  }

  // Extract the real error message from the context.
  // nix_err_msg returns a borrowed pointer valid until the next Nix call.
  // We must copy it to a String immediately.
  let msg = unsafe {
    let ptr = sys::nix_err_msg(std::ptr::null_mut(), ctx, std::ptr::null_mut());
    if ptr.is_null() {
      None
    } else {
      Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
  };

  // For NIX_ERR_NIX_ERROR, also try to get the richer info message.
  let detail = if err == sys::nix_err_NIX_ERR_NIX_ERROR {
    unsafe {
      string_from_callback(|cb, ud| {
        sys::nix_err_info_msg(std::ptr::null_mut(), ctx, cb, ud);
      })
    }
  } else {
    None
  };

  // Decorate with the symbolic error name (e.g. "NixError", "Key",
  // "Overflow") when the API exposes one for this code. Improves
  // diagnostics when the message itself is empty or generic.
  let name = unsafe {
    string_from_callback(|cb, ud| {
      sys::nix_err_name(std::ptr::null_mut(), ctx, cb, ud);
    })
  };

  let base_message = detail
    .or(msg)
    .unwrap_or_else(|| format!("Nix error code: {err}"));
  let message = match name {
    Some(n) if !n.is_empty() => format!("[{n}] {base_message}"),
    _ => base_message,
  };

  match err {
    sys::nix_err_NIX_ERR_UNKNOWN => Err(Error::Unknown(message)),
    sys::nix_err_NIX_ERR_OVERFLOW => Err(Error::Overflow),
    sys::nix_err_NIX_ERR_KEY => Err(Error::KeyNotFound(message)),
    sys::nix_err_NIX_ERR_NIX_ERROR => Err(Error::EvalError(message)),
    _ => Err(Error::Unknown(message)),
  }
}
