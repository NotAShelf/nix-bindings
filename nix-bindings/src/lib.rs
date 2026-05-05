#![warn(missing_docs)]
//! High-level, safe Rust bindings for the Nix build tool.
//!
//! This crate provides ergonomic and idiomatic Rust APIs for interacting
//! with Nix using its C API.
//!
//! # Quick Start
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use nix_bindings::{Context, EvalStateBuilder, Store};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let ctx = Arc::new(Context::new()?);
//! let store = Arc::new(Store::open(&ctx, None)?);
//! let state = EvalStateBuilder::new(&store)?.build()?;
//!
//! let result = state.eval_from_string("1 + 2", "<eval>")?;
//! println!("Result: {}", result.as_int()?);
//! # Ok(())
//! # }
//! ```
//!
//! # Value Formatting
//!
//! Values support multiple formatting options:
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use nix_bindings::{Context, EvalStateBuilder, Store};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let ctx = Arc::new(Context::new()?);
//! # let store = Arc::new(Store::open(&ctx, None)?);
//! # let state = EvalStateBuilder::new(&store)?.build()?;
//! let value = state.eval_from_string("\"hello world\"", "<eval>")?;
//!
//! // Display formatting (user-friendly)
//! println!("{}", value); // Output: hello world
//!
//! // Debug formatting (with type info)
//! println!("{:?}", value); // Output: Value::String("hello world")
//!
//! // Nix syntax formatting
//! println!("{}", value.to_nix_string()?); // Output: "hello world"
//! //
//! # Ok(())
//! # }
//! ```

mod attrs;
pub mod flake;
mod lists;
pub mod primop;
mod store;

use std::{
  ffi::{CStr, CString},
  fmt,
  path::Path,
  ptr::NonNull,
  sync::Arc,
};

#[cfg(test)] use serial_test::serial;
pub use store::{Derivation, Store, StorePath};

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
unsafe fn string_from_callback<F>(call: F) -> Option<String>
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
fn check_err(ctx: *mut sys::nix_c_context, err: sys::nix_err) -> Result<()> {
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

  let message = detail
    .or(msg)
    .unwrap_or_else(|| format!("Nix error code: {err}"));

  match err {
    sys::nix_err_NIX_ERR_UNKNOWN => Err(Error::Unknown(message)),
    sys::nix_err_NIX_ERR_OVERFLOW => Err(Error::Overflow),
    sys::nix_err_NIX_ERR_KEY => Err(Error::KeyNotFound(message)),
    sys::nix_err_NIX_ERR_NIX_ERROR => Err(Error::EvalError(message)),
    _ => Err(Error::Unknown(message)),
  }
}

/// Return the version of the Nix library being used.
///
/// This is a free function that does not require a context.
#[must_use]
pub fn nix_version() -> &'static str {
  // SAFETY: nix_version_get returns a pointer to a static string literal
  unsafe {
    let ptr = sys::nix_version_get();
    if ptr.is_null() {
      "<unknown>"
    } else {
      CStr::from_ptr(ptr).to_str().unwrap_or("<unknown>")
    }
  }
}

/// Verbosity level for Nix log output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
  /// Only errors.
  Error,
  /// Warnings and errors.
  Warn,
  /// Notices, warnings, and errors.
  Notice,
  /// Info messages.
  Info,
  /// Talkative output.
  Talkative,
  /// Chatty output.
  Chatty,
  /// Debug output.
  Debug,
  /// Maximum verbosity (vomit).
  Vomit,
}

impl Verbosity {
  fn to_c(self) -> sys::nix_verbosity {
    match self {
      Verbosity::Error => sys::nix_verbosity_NIX_LVL_ERROR,
      Verbosity::Warn => sys::nix_verbosity_NIX_LVL_WARN,
      Verbosity::Notice => sys::nix_verbosity_NIX_LVL_NOTICE,
      Verbosity::Info => sys::nix_verbosity_NIX_LVL_INFO,
      Verbosity::Talkative => sys::nix_verbosity_NIX_LVL_TALKATIVE,
      Verbosity::Chatty => sys::nix_verbosity_NIX_LVL_CHATTY,
      Verbosity::Debug => sys::nix_verbosity_NIX_LVL_DEBUG,
      Verbosity::Vomit => sys::nix_verbosity_NIX_LVL_VOMIT,
    }
  }
}

/// Nix context for managing library state.
///
/// This is the root object for all Nix operations. It manages the lifetime
/// of the Nix C API context and provides automatic cleanup.
pub struct Context {
  inner: NonNull<sys::nix_c_context>,
}

impl Context {
  /// Create a new Nix context.
  ///
  /// This initializes the Nix C API context and the required libraries.
  ///
  /// # Errors
  ///
  /// Returns an error if context creation or library initialization fails.
  pub fn new() -> Result<Self> {
    // SAFETY: nix_c_context_create is safe to call
    let ctx_ptr = unsafe { sys::nix_c_context_create() };
    let inner = NonNull::new(ctx_ptr).ok_or(Error::NullPointer)?;

    let ctx = Context { inner };

    // Initialize required libraries
    unsafe {
      check_err(
        ctx.inner.as_ptr(),
        sys::nix_libutil_init(ctx.inner.as_ptr()),
      )?;
      check_err(
        ctx.inner.as_ptr(),
        sys::nix_libstore_init(ctx.inner.as_ptr()),
      )?;
      check_err(
        ctx.inner.as_ptr(),
        sys::nix_libexpr_init(ctx.inner.as_ptr()),
      )?;
    }

    Ok(ctx)
  }

  /// Set a global Nix configuration setting.
  ///
  /// Settings take effect for new [`EvalState`] instances. Use
  /// `"extra-<name>"` to append to an existing setting's value.
  ///
  /// # Errors
  ///
  /// Returns [`Error::KeyNotFound`] if the setting key is unknown.
  pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
    let key_c = CString::new(key)?;
    let value_c = CString::new(value)?;
    // SAFETY: context and strings are valid
    unsafe {
      check_err(
        self.inner.as_ptr(),
        sys::nix_setting_set(
          self.inner.as_ptr(),
          key_c.as_ptr(),
          value_c.as_ptr(),
        ),
      )
    }
  }

  /// Get the value of a global Nix configuration setting.
  ///
  /// # Errors
  ///
  /// Returns [`Error::KeyNotFound`] if the setting key is unknown.
  pub fn get_setting(&self, key: &str) -> Result<String> {
    let key_c = CString::new(key)?;
    let mut err_code = sys::nix_err_NIX_OK;
    // SAFETY: context and key are valid
    let result = unsafe {
      string_from_callback(|cb, ud| {
        err_code =
          sys::nix_setting_get(self.inner.as_ptr(), key_c.as_ptr(), cb, ud);
      })
    };
    check_err(self.inner.as_ptr(), err_code)?;
    result.ok_or_else(|| Error::KeyNotFound(key.to_string()))
  }

  /// Set the verbosity level for Nix log output.
  ///
  /// # Errors
  ///
  /// Returns an error if the verbosity level cannot be set.
  pub fn set_verbosity(&self, level: Verbosity) -> Result<()> {
    // SAFETY: context is valid
    unsafe {
      check_err(
        self.inner.as_ptr(),
        sys::nix_set_verbosity(self.inner.as_ptr(), level.to_c()),
      )
    }
  }

  /// Get the raw context pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::nix_c_context {
    self.inner.as_ptr()
  }
}

impl Drop for Context {
  fn drop(&mut self) {
    // SAFETY: We own the context and it's valid until drop
    unsafe {
      sys::nix_c_context_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: Context can be shared between threads
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

/// Builder for Nix evaluation state.
///
/// This allows configuring the evaluation environment before creating
/// the evaluation state.
pub struct EvalStateBuilder {
  inner:   NonNull<sys::nix_eval_state_builder>,
  store:   Arc<Store>,
  context: Arc<Context>,
}

impl EvalStateBuilder {
  /// Create a new evaluation state builder.
  ///
  /// # Arguments
  ///
  /// * `store` - The Nix store to use for evaluation
  ///
  /// # Errors
  ///
  /// Returns an error if the builder cannot be created.
  pub fn new(store: &Arc<Store>) -> Result<Self> {
    // SAFETY: store context and store are valid
    let builder_ptr = unsafe {
      sys::nix_eval_state_builder_new(store._context.as_ptr(), store.as_ptr())
    };

    let inner = NonNull::new(builder_ptr).ok_or(Error::NullPointer)?;

    Ok(EvalStateBuilder {
      inner,
      store: Arc::clone(store),
      context: Arc::clone(&store._context),
    })
  }

  /// Set the lookup path (`NIX_PATH`) for `<...>` expressions.
  ///
  /// Each entry should be in the form `"name=path"` or just `"path"`,
  /// matching the format of `NIX_PATH` entries.
  ///
  /// # Errors
  ///
  /// Returns an error if the lookup path cannot be set.
  pub fn set_lookup_path(self, paths: &[impl AsRef<str>]) -> Result<Self> {
    // Build null-terminated array of C strings.
    let c_strings: Vec<CString> = paths
      .iter()
      .map(|s| CString::new(s.as_ref()))
      .collect::<std::result::Result<_, _>>()?;

    let mut ptrs: Vec<*const std::os::raw::c_char> =
      c_strings.iter().map(|cs| cs.as_ptr()).collect();
    ptrs.push(std::ptr::null()); // null terminator

    // SAFETY: context and builder are valid, ptrs is null-terminated
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_eval_state_builder_set_lookup_path(
          self.context.as_ptr(),
          self.inner.as_ptr(),
          ptrs.as_mut_ptr(),
        ),
      )?;
    }

    Ok(self)
  }

  /// Apply flake settings to the evaluation state builder.
  ///
  /// This enables `builtins.getFlake` and related flake functionality
  /// in the resulting [`EvalState`].
  ///
  /// # Errors
  ///
  /// Returns an error if the flake settings cannot be applied.
  pub fn with_flake_settings(
    self,
    settings: &flake::FlakeSettings,
  ) -> Result<Self> {
    // SAFETY: context, settings, and builder are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_flake_settings_add_to_eval_state_builder(
          self.context.as_ptr(),
          settings.as_ptr(),
          self.inner.as_ptr(),
        ),
      )?;
    }

    Ok(self)
  }

  /// Build the evaluation state.
  ///
  /// # Errors
  ///
  /// Returns an error if the evaluation state cannot be built.
  pub fn build(self) -> Result<EvalState> {
    // Load configuration from environment first
    // SAFETY: context and builder are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_eval_state_builder_load(
          self.context.as_ptr(),
          self.inner.as_ptr(),
        ),
      )?;
    }

    // Build the state
    // SAFETY: context and builder are valid
    let state_ptr = unsafe {
      sys::nix_eval_state_build(self.context.as_ptr(), self.inner.as_ptr())
    };

    let inner = NonNull::new(state_ptr).ok_or(Error::NullPointer)?;

    Ok(EvalState {
      inner,
      store: self.store.clone(),
      context: self.context.clone(),
    })
  }
}

impl Drop for EvalStateBuilder {
  fn drop(&mut self) {
    // SAFETY: We own the builder and it's valid until drop
    unsafe {
      sys::nix_eval_state_builder_free(self.inner.as_ptr());
    }
  }
}

/// Nix evaluation state for evaluating expressions.
///
/// This provides the main interface for evaluating Nix expressions
/// and creating values.
pub struct EvalState {
  pub(crate) inner:   NonNull<sys::EvalState>,
  #[allow(dead_code)]
  store:              Arc<Store>,
  pub(crate) context: Arc<Context>,
}

impl EvalState {
  /// Evaluate a Nix expression from a string.
  ///
  /// # Arguments
  ///
  /// * `expr` - The Nix expression to evaluate
  /// * `path` - The path to use for error reporting (e.g., `"<eval>"`)
  ///
  /// # Errors
  ///
  /// Returns an error if evaluation fails.
  pub fn eval_from_string(&self, expr: &str, path: &str) -> Result<Value<'_>> {
    let expr_c = CString::new(expr)?;
    let path_c = CString::new(path)?;

    // Allocate value for result
    // SAFETY: context and state are valid
    let value_ptr = unsafe {
      sys::nix_alloc_value(self.context.as_ptr(), self.inner.as_ptr())
    };
    if value_ptr.is_null() {
      return Err(Error::NullPointer);
    }

    // Evaluate expression
    // SAFETY: all pointers are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_expr_eval_from_string(
          self.context.as_ptr(),
          self.inner.as_ptr(),
          expr_c.as_ptr(),
          path_c.as_ptr(),
          value_ptr,
        ),
      )?;
    }

    let inner = NonNull::new(value_ptr).ok_or(Error::NullPointer)?;

    Ok(Value { inner, state: self })
  }

  /// Allocate a new uninitialized value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation fails.
  pub fn alloc_value(&self) -> Result<Value<'_>> {
    // SAFETY: context and state are valid
    let value_ptr = unsafe {
      sys::nix_alloc_value(self.context.as_ptr(), self.inner.as_ptr())
    };
    let inner = NonNull::new(value_ptr).ok_or(Error::NullPointer)?;

    Ok(Value { inner, state: self })
  }

  /// Create a Nix integer value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation or initialization fails.
  pub fn make_int(&self, i: i64) -> Result<Value<'_>> {
    let v = self.alloc_value()?;
    // SAFETY: context and value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_init_int(self.context.as_ptr(), v.inner.as_ptr(), i),
      )?;
    }
    Ok(v)
  }

  /// Create a Nix float value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation or initialization fails.
  pub fn make_float(&self, f: f64) -> Result<Value<'_>> {
    let v = self.alloc_value()?;
    // SAFETY: context and value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_init_float(self.context.as_ptr(), v.inner.as_ptr(), f),
      )?;
    }
    Ok(v)
  }

  /// Create a Nix boolean value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation or initialization fails.
  pub fn make_bool(&self, b: bool) -> Result<Value<'_>> {
    let v = self.alloc_value()?;
    // SAFETY: context and value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_init_bool(self.context.as_ptr(), v.inner.as_ptr(), b),
      )?;
    }
    Ok(v)
  }

  /// Create a Nix null value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation or initialization fails.
  pub fn make_null(&self) -> Result<Value<'_>> {
    let v = self.alloc_value()?;
    // SAFETY: context and value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_init_null(self.context.as_ptr(), v.inner.as_ptr()),
      )?;
    }
    Ok(v)
  }

  /// Create a Nix string value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation, string conversion, or
  /// initialization fails.
  pub fn make_string(&self, s: &str) -> Result<Value<'_>> {
    let v = self.alloc_value()?;
    let s_c = CString::new(s)?;
    // SAFETY: context and value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_init_string(
          self.context.as_ptr(),
          v.inner.as_ptr(),
          s_c.as_ptr(),
        ),
      )?;
    }
    Ok(v)
  }

  /// Create a Nix path value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation, path conversion, or
  /// initialization fails.
  pub fn make_path(&self, path: impl AsRef<Path>) -> Result<Value<'_>> {
    let v = self.alloc_value()?;
    let path_str = path
      .as_ref()
      .to_str()
      .ok_or_else(|| Error::Unknown("Path is not valid UTF-8".to_string()))?;
    let path_c = CString::new(path_str)?;
    // SAFETY: context, state, and value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_init_path_string(
          self.context.as_ptr(),
          self.inner.as_ptr(),
          v.inner.as_ptr(),
          path_c.as_ptr(),
        ),
      )?;
    }
    Ok(v)
  }

  /// Create a Nix list value from a slice of values.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation or list construction fails.
  pub fn make_list(&self, items: &[&Value<'_>]) -> Result<Value<'_>> {
    // SAFETY: context and state are valid
    let builder = unsafe {
      sys::nix_make_list_builder(
        self.context.as_ptr(),
        self.inner.as_ptr(),
        items.len(),
      )
    };
    if builder.is_null() {
      return Err(Error::NullPointer);
    }

    // Free the builder on all paths, including early returns on error.
    struct ListBuilderGuard(*mut sys::ListBuilder);
    impl Drop for ListBuilderGuard {
      fn drop(&mut self) {
        unsafe { sys::nix_list_builder_free(self.0) };
      }
    }
    let _guard = ListBuilderGuard(builder);

    // Insert each item
    for (i, item) in items.iter().enumerate() {
      // SAFETY: context, builder, and value are valid; index is in bounds
      unsafe {
        check_err(
          self.context.as_ptr(),
          sys::nix_list_builder_insert(
            self.context.as_ptr(),
            builder,
            i as std::os::raw::c_uint,
            item.inner.as_ptr(),
          ),
        )?;
      }
    }

    let result = self.alloc_value()?;
    // SAFETY: context, builder, and result value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_make_list(
          self.context.as_ptr(),
          builder,
          result.inner.as_ptr(),
        ),
      )?;
    }

    Ok(result)
  }

  /// Create a Nix attribute set from key-value pairs.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation or attribute set construction fails.
  pub fn make_attrs<'s>(
    &'s self,
    pairs: &[(&str, &Value<'_>)],
  ) -> Result<Value<'s>> {
    // SAFETY: context and state are valid
    let builder = unsafe {
      sys::nix_make_bindings_builder(
        self.context.as_ptr(),
        self.inner.as_ptr(),
        pairs.len(),
      )
    };
    if builder.is_null() {
      return Err(Error::NullPointer);
    }

    // Free the builder on all paths, including early returns on error.
    struct BindingsBuilderGuard(*mut sys::BindingsBuilder);
    impl Drop for BindingsBuilderGuard {
      fn drop(&mut self) {
        unsafe { sys::nix_bindings_builder_free(self.0) };
      }
    }
    let _guard = BindingsBuilderGuard(builder);

    // Insert each key-value pair
    for (key, value) in pairs {
      let key_c = CString::new(*key)?;
      // SAFETY: context, builder, key, and value are valid
      unsafe {
        check_err(
          self.context.as_ptr(),
          sys::nix_bindings_builder_insert(
            self.context.as_ptr(),
            builder,
            key_c.as_ptr(),
            value.inner.as_ptr(),
          ),
        )?;
      }
    }

    let result = self.alloc_value()?;
    // SAFETY: context, builder, and result value are valid
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_make_attrs(
          self.context.as_ptr(),
          result.inner.as_ptr(),
          builder,
        ),
      )?;
    }

    Ok(result)
  }

  /// Get the raw state pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::EvalState {
    self.inner.as_ptr()
  }
}

impl Drop for EvalState {
  fn drop(&mut self) {
    // SAFETY: We own the state and it's valid until drop
    unsafe {
      sys::nix_state_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: EvalState can be shared between threads
unsafe impl Send for EvalState {}
unsafe impl Sync for EvalState {}

/// Nix value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
  /// Thunk (unevaluated expression).
  Thunk,
  /// Integer value.
  Int,
  /// Float value.
  Float,
  /// Boolean value.
  Bool,
  /// String value.
  String,
  /// Path value.
  Path,
  /// Null value.
  Null,
  /// Attribute set.
  Attrs,
  /// List.
  List,
  /// Function.
  Function,
  /// External value.
  External,
}

impl ValueType {
  fn from_c(value_type: sys::ValueType) -> Self {
    match value_type {
      sys::ValueType_NIX_TYPE_THUNK => ValueType::Thunk,
      sys::ValueType_NIX_TYPE_INT => ValueType::Int,
      sys::ValueType_NIX_TYPE_FLOAT => ValueType::Float,
      sys::ValueType_NIX_TYPE_BOOL => ValueType::Bool,
      sys::ValueType_NIX_TYPE_STRING => ValueType::String,
      sys::ValueType_NIX_TYPE_PATH => ValueType::Path,
      sys::ValueType_NIX_TYPE_NULL => ValueType::Null,
      sys::ValueType_NIX_TYPE_ATTRS => ValueType::Attrs,
      sys::ValueType_NIX_TYPE_LIST => ValueType::List,
      sys::ValueType_NIX_TYPE_FUNCTION => ValueType::Function,
      sys::ValueType_NIX_TYPE_EXTERNAL => ValueType::External,
      _ => ValueType::Thunk, // fallback
    }
  }
}

impl fmt::Display for ValueType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let name = match self {
      ValueType::Thunk => "thunk",
      ValueType::Int => "int",
      ValueType::Float => "float",
      ValueType::Bool => "bool",
      ValueType::String => "string",
      ValueType::Path => "path",
      ValueType::Null => "null",
      ValueType::Attrs => "attrs",
      ValueType::List => "list",
      ValueType::Function => "function",
      ValueType::External => "external",
    };
    write!(f, "{name}")
  }
}

/// A Nix value.
///
/// This represents any value in the Nix language, including primitives,
/// collections, and functions. Values are GC-managed; this struct holds
/// a reference count that is released on drop.
pub struct Value<'a> {
  pub(crate) inner: NonNull<sys::nix_value>,
  pub(crate) state: &'a EvalState,
}

impl Value<'_> {
  /// Force evaluation of this value.
  ///
  /// If the value is a thunk, this will evaluate it to its final form.
  ///
  /// # Errors
  ///
  /// Returns an error if evaluation fails.
  pub fn force(&mut self) -> Result<()> {
    // SAFETY: context, state, and value are valid
    unsafe {
      check_err(
        self.state.context.as_ptr(),
        sys::nix_value_force(
          self.state.context.as_ptr(),
          self.state.as_ptr(),
          self.inner.as_ptr(),
        ),
      )
    }
  }

  /// Force deep evaluation of this value.
  ///
  /// This forces evaluation of the value and all its nested components.
  ///
  /// # Errors
  ///
  /// Returns an error if evaluation fails.
  pub fn force_deep(&mut self) -> Result<()> {
    // SAFETY: context, state, and value are valid
    unsafe {
      check_err(
        self.state.context.as_ptr(),
        sys::nix_value_force_deep(
          self.state.context.as_ptr(),
          self.state.as_ptr(),
          self.inner.as_ptr(),
        ),
      )
    }
  }

  /// Get the type of this value.
  #[must_use]
  pub fn value_type(&self) -> ValueType {
    // SAFETY: context and value are valid
    let c_type = unsafe {
      sys::nix_get_type(self.state.context.as_ptr(), self.inner.as_ptr())
    };
    ValueType::from_c(c_type)
  }

  /// Convert this value to an integer.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not an integer.
  pub fn as_int(&self) -> Result<i64> {
    if self.value_type() != ValueType::Int {
      return Err(Error::InvalidType {
        expected: "int",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: context and value are valid, type is checked
    let result = unsafe {
      sys::nix_get_int(self.state.context.as_ptr(), self.inner.as_ptr())
    };

    Ok(result)
  }

  /// Convert this value to a float.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not a float.
  pub fn as_float(&self) -> Result<f64> {
    if self.value_type() != ValueType::Float {
      return Err(Error::InvalidType {
        expected: "float",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: context and value are valid, type is checked
    let result = unsafe {
      sys::nix_get_float(self.state.context.as_ptr(), self.inner.as_ptr())
    };

    Ok(result)
  }

  /// Convert this value to a boolean.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not a boolean.
  pub fn as_bool(&self) -> Result<bool> {
    if self.value_type() != ValueType::Bool {
      return Err(Error::InvalidType {
        expected: "bool",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: context and value are valid, type is checked
    let result = unsafe {
      sys::nix_get_bool(self.state.context.as_ptr(), self.inner.as_ptr())
    };

    Ok(result)
  }

  /// Convert this value to a string.
  ///
  /// This realises the string (resolves any context/store paths) and
  /// returns its content.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not a string.
  pub fn as_string(&self) -> Result<String> {
    if self.value_type() != ValueType::String {
      return Err(Error::InvalidType {
        expected: "string",
        actual:   self.value_type().to_string(),
      });
    }

    // Use the realised string API to handle string contexts correctly.
    // SAFETY: context, state, and value are valid; type is checked
    let realised_str = unsafe {
      sys::nix_string_realise(
        self.state.context.as_ptr(),
        self.state.as_ptr(),
        self.inner.as_ptr(),
        false,
      )
    };

    if realised_str.is_null() {
      return Err(Error::NullPointer);
    }

    // SAFETY: realised_str is non-null
    let buffer_start =
      unsafe { sys::nix_realised_string_get_buffer_start(realised_str) };

    let buffer_size =
      unsafe { sys::nix_realised_string_get_buffer_size(realised_str) };

    if buffer_start.is_null() {
      unsafe { sys::nix_realised_string_free(realised_str) };
      return Err(Error::NullPointer);
    }

    // SAFETY: buffer_start and buffer_size are valid
    let bytes = unsafe {
      std::slice::from_raw_parts(buffer_start.cast::<u8>(), buffer_size)
    };
    let string = std::str::from_utf8(bytes)
      .map_err(|_| Error::Unknown("Invalid UTF-8 in string".to_string()))?
      .to_owned();

    unsafe { sys::nix_realised_string_free(realised_str) };

    Ok(string)
  }

  /// Convert this value to a filesystem path.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not a path.
  pub fn as_path(&self) -> Result<std::path::PathBuf> {
    if self.value_type() != ValueType::Path {
      return Err(Error::InvalidType {
        expected: "path",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: context and value are valid, type is checked
    let path_ptr = unsafe {
      sys::nix_get_path_string(self.state.context.as_ptr(), self.inner.as_ptr())
    };

    if path_ptr.is_null() {
      return Err(Error::NullPointer);
    }

    // SAFETY: path_ptr is a valid C string
    let path_str =
      unsafe { CStr::from_ptr(path_ptr).to_string_lossy().into_owned() };

    Ok(std::path::PathBuf::from(path_str))
  }

  /// Call this value as a function with a single argument.
  ///
  /// # Errors
  ///
  /// Returns an error if this value is not a function or the call fails.
  pub fn call(&self, arg: &Value<'_>) -> Result<Value<'_>> {
    let result = self.state.alloc_value()?;
    // SAFETY: context, state, function value, arg value, and result are valid
    unsafe {
      check_err(
        self.state.context.as_ptr(),
        sys::nix_value_call(
          self.state.context.as_ptr(),
          self.state.as_ptr(),
          self.inner.as_ptr(),
          arg.inner.as_ptr(),
          result.inner.as_ptr(),
        ),
      )?;
    }
    Ok(result)
  }

  /// Call this value as a curried function with multiple arguments.
  ///
  /// # Errors
  ///
  /// Returns an error if this value is not a function or the call fails.
  pub fn call_multi(&self, args: &[&Value<'_>]) -> Result<Value<'_>> {
    let result = self.state.alloc_value()?;
    let mut arg_ptrs: Vec<*mut sys::nix_value> =
      args.iter().map(|a| a.inner.as_ptr()).collect();
    // SAFETY: context, state, fn, args array, and result are valid
    unsafe {
      check_err(
        self.state.context.as_ptr(),
        sys::nix_value_call_multi(
          self.state.context.as_ptr(),
          self.state.as_ptr(),
          self.inner.as_ptr(),
          arg_ptrs.len(),
          arg_ptrs.as_mut_ptr(),
          result.inner.as_ptr(),
        ),
      )?;
    }
    Ok(result)
  }

  /// Create a lazy thunk that applies a function to an argument.
  ///
  /// Unlike [`call`](Self::call), this does not perform the call immediately;
  /// it stores it as a thunk to be evaluated lazily. This is useful for
  /// constructing lazy attribute sets and lists.
  ///
  /// # Errors
  ///
  /// Returns an error if the thunk cannot be created.
  pub fn make_thunk<'a>(
    fn_val: &'a Value<'a>,
    arg: &'a Value<'a>,
  ) -> Result<Value<'a>> {
    let result = fn_val.state.alloc_value()?;
    // SAFETY: context and all value pointers are valid
    unsafe {
      check_err(
        fn_val.state.context.as_ptr(),
        sys::nix_init_apply(
          fn_val.state.context.as_ptr(),
          result.inner.as_ptr(),
          fn_val.inner.as_ptr(),
          arg.inner.as_ptr(),
        ),
      )?;
    }
    Ok(result)
  }

  /// Copy this value into a new owned value.
  ///
  /// # Errors
  ///
  /// Returns an error if the copy fails.
  pub fn copy(&self) -> Result<Value<'_>> {
    let result = self.state.alloc_value()?;
    // SAFETY: context and both value pointers are valid
    unsafe {
      check_err(
        self.state.context.as_ptr(),
        sys::nix_copy_value(
          self.state.context.as_ptr(),
          result.inner.as_ptr(),
          self.inner.as_ptr(),
        ),
      )?;
    }
    Ok(result)
  }

  /// Get the raw value pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  #[allow(dead_code)]
  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::nix_value {
    self.inner.as_ptr()
  }

  /// Format this value as Nix syntax.
  ///
  /// This provides a string representation that matches Nix's own syntax,
  /// making it useful for debugging and displaying values to users.
  ///
  /// # Errors
  ///
  /// Returns an error if the value cannot be converted to a string
  /// representation.
  pub fn to_nix_string(&self) -> Result<String> {
    match self.value_type() {
      ValueType::Int => Ok(self.as_int()?.to_string()),
      ValueType::Float => Ok(self.as_float()?.to_string()),
      ValueType::Bool => {
        Ok(if self.as_bool()? {
          "true".to_string()
        } else {
          "false".to_string()
        })
      },
      ValueType::String => {
        Ok(format!("\"{}\"", self.as_string()?.replace('"', "\\\"")))
      },
      ValueType::Null => Ok("null".to_string()),
      ValueType::Attrs => Ok("{ <attrs> }".to_string()),
      ValueType::List => Ok("[ <list> ]".to_string()),
      ValueType::Function => Ok("<function>".to_string()),
      ValueType::Path => {
        Ok(
          self
            .as_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<path>".to_string()),
        )
      },
      ValueType::Thunk => Ok("<thunk>".to_string()),
      ValueType::External => Ok("<external>".to_string()),
    }
  }
}

impl Drop for Value<'_> {
  fn drop(&mut self) {
    // SAFETY: We hold a GC reference (automatically incremented for us by
    // the Nix C API when the value was returned). Release it here.
    unsafe {
      sys::nix_value_decref(self.state.context.as_ptr(), self.inner.as_ptr());
    }
  }
}

impl fmt::Display for Value<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.value_type() {
      ValueType::Int => {
        if let Ok(val) = self.as_int() {
          write!(f, "{val}")
        } else {
          write!(f, "<int error>")
        }
      },
      ValueType::Float => {
        if let Ok(val) = self.as_float() {
          write!(f, "{val}")
        } else {
          write!(f, "<float error>")
        }
      },
      ValueType::Bool => {
        if let Ok(val) = self.as_bool() {
          write!(f, "{val}")
        } else {
          write!(f, "<bool error>")
        }
      },
      ValueType::String => {
        if let Ok(val) = self.as_string() {
          write!(f, "{val}")
        } else {
          write!(f, "<string error>")
        }
      },
      ValueType::Null => write!(f, "null"),
      ValueType::Attrs => write!(f, "{{ <attrs> }}"),
      ValueType::List => write!(f, "[ <list> ]"),
      ValueType::Function => write!(f, "<function>"),
      ValueType::Path => {
        if let Ok(p) = self.as_path() {
          write!(f, "{}", p.display())
        } else {
          write!(f, "<path>")
        }
      },
      ValueType::Thunk => write!(f, "<thunk>"),
      ValueType::External => write!(f, "<external>"),
    }
  }
}

impl fmt::Debug for Value<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let value_type = self.value_type();
    match value_type {
      ValueType::Int => {
        if let Ok(val) = self.as_int() {
          write!(f, "Value::Int({val})")
        } else {
          write!(f, "Value::Int(<error>)")
        }
      },
      ValueType::Float => {
        if let Ok(val) = self.as_float() {
          write!(f, "Value::Float({val})")
        } else {
          write!(f, "Value::Float(<error>)")
        }
      },
      ValueType::Bool => {
        if let Ok(val) = self.as_bool() {
          write!(f, "Value::Bool({val})")
        } else {
          write!(f, "Value::Bool(<error>)")
        }
      },
      ValueType::String => {
        if let Ok(val) = self.as_string() {
          write!(f, "Value::String({val:?})")
        } else {
          write!(f, "Value::String(<error>)")
        }
      },
      ValueType::Null => write!(f, "Value::Null"),
      ValueType::Attrs => write!(f, "Value::Attrs({{ <attrs> }})"),
      ValueType::List => write!(f, "Value::List([ <list> ])"),
      ValueType::Function => write!(f, "Value::Function(<function>)"),
      ValueType::Path => {
        if let Ok(p) = self.as_path() {
          write!(f, "Value::Path({})", p.display())
        } else {
          write!(f, "Value::Path(<path>)")
        }
      },
      ValueType::Thunk => write!(f, "Value::Thunk(<thunk>)"),
      ValueType::External => write!(f, "Value::External(<external>)"),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[serial]
  fn test_context_creation() {
    let _ctx = Context::new().expect("Failed to create context");
    // Context should be dropped automatically
  }

  #[test]
  #[serial]
  fn test_nix_version() {
    let version = nix_version();
    assert!(!version.is_empty(), "Version should not be empty");
  }

  #[test]
  #[serial]
  fn test_eval_state_builder() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let _state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");
    // State should be dropped automatically
  }

  #[test]
  #[serial]
  fn test_simple_evaluation() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let result = state
      .eval_from_string("1 + 2", "<eval>")
      .expect("Failed to evaluate expression");

    assert_eq!(result.value_type(), ValueType::Int);
    assert_eq!(result.as_int().expect("Failed to get int value"), 3);
  }

  #[test]
  #[serial]
  fn test_value_types() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    // Test integer
    let int_val = state
      .eval_from_string("42", "<eval>")
      .expect("Failed to evaluate int");
    assert_eq!(int_val.value_type(), ValueType::Int);
    assert_eq!(int_val.as_int().expect("Failed to get int"), 42);

    // Test boolean
    let bool_val = state
      .eval_from_string("true", "<eval>")
      .expect("Failed to evaluate bool");
    assert_eq!(bool_val.value_type(), ValueType::Bool);
    assert!(bool_val.as_bool().expect("Failed to get bool"));

    // Test string
    let str_val = state
      .eval_from_string("\"hello\"", "<eval>")
      .expect("Failed to evaluate string");
    assert_eq!(str_val.value_type(), ValueType::String);
    assert_eq!(str_val.as_string().expect("Failed to get string"), "hello");
  }

  #[test]
  #[serial]
  fn test_value_construction() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let int_val = state.make_int(99).expect("Failed to make int");
    assert_eq!(int_val.as_int().unwrap(), 99);

    let float_val = state.make_float(2.5).expect("Failed to make float");
    assert!((float_val.as_float().unwrap() - 2.5).abs() < 1e-9);

    let bool_val = state.make_bool(true).expect("Failed to make bool");
    assert!(bool_val.as_bool().unwrap());

    let null_val = state.make_null().expect("Failed to make null");
    assert_eq!(null_val.value_type(), ValueType::Null);

    let str_val = state.make_string("hello").expect("Failed to make string");
    assert_eq!(str_val.as_string().unwrap(), "hello");
  }

  #[test]
  #[serial]
  fn test_make_list() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let a = state.make_int(1).unwrap();
    let b = state.make_int(2).unwrap();
    let c = state.make_int(3).unwrap();

    let list = state.make_list(&[&a, &b, &c]).expect("Failed to make list");
    assert_eq!(list.value_type(), ValueType::List);
    assert_eq!(list.list_len().unwrap(), 3);
  }

  #[test]
  #[serial]
  fn test_make_attrs() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let a = state.make_int(42).unwrap();
    let b = state.make_string("hello").unwrap();

    let attrs = state
      .make_attrs(&[("answer", &a), ("greeting", &b)])
      .expect("Failed to make attrs");
    assert_eq!(attrs.value_type(), ValueType::Attrs);

    let mut answer = attrs.get_attr("answer").unwrap();
    answer.force().unwrap();
    assert_eq!(answer.as_int().unwrap(), 42);
  }

  #[test]
  #[serial]
  fn test_value_call() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let f = state
      .eval_from_string("x: x + 1", "<eval>")
      .expect("Failed to evaluate function");
    let arg = state.make_int(41).unwrap();
    let result = f.call(&arg).expect("Failed to call function");
    assert_eq!(result.as_int().unwrap(), 42);
  }

  #[test]
  #[serial]
  fn test_value_copy() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let orig = state.make_int(7).unwrap();
    let copy = orig.copy().expect("Failed to copy value");
    assert_eq!(copy.as_int().unwrap(), 7);
  }

  #[test]
  #[serial]
  fn test_value_formatting() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    // Test integer formatting
    let int_val = state
      .eval_from_string("42", "<eval>")
      .expect("Failed to evaluate int");
    assert_eq!(format!("{int_val}"), "42");
    assert_eq!(format!("{int_val:?}"), "Value::Int(42)");
    assert_eq!(int_val.to_nix_string().expect("Failed to format"), "42");

    // Test boolean formatting
    let bool_val = state
      .eval_from_string("true", "<eval>")
      .expect("Failed to evaluate bool");
    assert_eq!(format!("{bool_val}"), "true");
    assert_eq!(format!("{bool_val:?}"), "Value::Bool(true)");
    assert_eq!(bool_val.to_nix_string().expect("Failed to format"), "true");

    let false_val = state
      .eval_from_string("false", "<eval>")
      .expect("Failed to evaluate bool");
    assert_eq!(format!("{false_val}"), "false");
    assert_eq!(
      false_val.to_nix_string().expect("Failed to format"),
      "false"
    );

    // Test string formatting
    let str_val = state
      .eval_from_string("\"hello world\"", "<eval>")
      .expect("Failed to evaluate string");
    assert_eq!(format!("{str_val}"), "hello world");
    assert_eq!(format!("{str_val:?}"), "Value::String(\"hello world\")");
    assert_eq!(
      str_val.to_nix_string().expect("Failed to format"),
      "\"hello world\""
    );

    // Test string with quotes
    let quoted_str = state
      .eval_from_string("\"say \\\"hello\\\"\"", "<eval>")
      .expect("Failed to evaluate quoted string");
    assert_eq!(format!("{quoted_str}"), "say \"hello\"");
    assert_eq!(
      quoted_str.to_nix_string().expect("Failed to format"),
      "\"say \\\"hello\\\"\""
    );

    // Test null formatting
    let null_val = state
      .eval_from_string("null", "<eval>")
      .expect("Failed to evaluate null");
    assert_eq!(format!("{null_val}"), "null");
    assert_eq!(format!("{null_val:?}"), "Value::Null");
    assert_eq!(null_val.to_nix_string().expect("Failed to format"), "null");

    // Test collection formatting
    let attrs_val = state
      .eval_from_string("{ a = 1; }", "<eval>")
      .expect("Failed to evaluate attrs");
    assert_eq!(format!("{attrs_val}"), "{ <attrs> }");
    assert_eq!(format!("{attrs_val:?}"), "Value::Attrs({ <attrs> })");

    let list_val = state
      .eval_from_string("[ 1 2 3 ]", "<eval>")
      .expect("Failed to evaluate list");
    assert_eq!(format!("{list_val}"), "[ <list> ]");
    assert_eq!(format!("{list_val:?}"), "Value::List([ <list> ])");
  }
}
