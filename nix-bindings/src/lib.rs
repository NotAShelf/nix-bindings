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

mod store;

pub use store::Store;

use std::{ffi::CString, fmt, ptr::NonNull, sync::Arc};

#[cfg(test)] use serial_test::serial;

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

/// Check a Nix error code and convert to Result.
fn check_err(err: sys::nix_err) -> Result<()> {
  match err {
    sys::nix_err_NIX_OK => Ok(()),
    sys::nix_err_NIX_ERR_UNKNOWN => {
      Err(Error::Unknown("Unknown error".to_string()))
    },
    sys::nix_err_NIX_ERR_OVERFLOW => Err(Error::Overflow),
    sys::nix_err_NIX_ERR_KEY => {
      Err(Error::KeyNotFound("Key not found".to_string()))
    },
    sys::nix_err_NIX_ERR_NIX_ERROR => {
      Err(Error::EvalError("Evaluation error".to_string()))
    },
    _ => Err(Error::Unknown(format!("Error code: {err}"))),
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
      check_err(sys::nix_libutil_init(ctx.inner.as_ptr()))?;
      check_err(sys::nix_libstore_init(ctx.inner.as_ptr()))?;
      check_err(sys::nix_libexpr_init(ctx.inner.as_ptr()))?;
    }

    Ok(ctx)
  }

  /// Get the raw context pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  unsafe fn as_ptr(&self) -> *mut sys::nix_c_context {
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

  /// Build the evaluation state.
  ///
  /// # Errors
  ///
  /// Returns an error if the evaluation state cannot be built.
  pub fn build(self) -> Result<EvalState> {
    // Load configuration first
    // SAFETY: context and builder are valid
    unsafe {
      check_err(sys::nix_eval_state_builder_load(
        self.context.as_ptr(),
        self.inner.as_ptr(),
      ))?;
    }

    // Build the state
    // SAFETY: context and builder are valid
    let state_ptr = unsafe {
      sys::nix_eval_state_build(self.context.as_ptr(), self.inner.as_ptr())
    };

    let inner = NonNull::new(state_ptr).ok_or(Error::NullPointer)?;

    // The builder is consumed here - its Drop will clean up
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
  inner:   NonNull<sys::EvalState>,
  #[allow(dead_code)]
  store:   Arc<Store>,
  context: Arc<Context>,
}

impl EvalState {
  /// Evaluate a Nix expression from a string.
  ///
  /// # Arguments
  ///
  /// * `expr` - The Nix expression to evaluate
  /// * `path` - The path to use for error reporting (e.g., "<eval>")
  ///
  /// # Errors
  ///
  /// Returns an error if evaluation fails.
  pub fn eval_from_string(&self, expr: &str, path: &str) -> Result<Value> {
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
      check_err(sys::nix_expr_eval_from_string(
        self.context.as_ptr(),
        self.inner.as_ptr(),
        expr_c.as_ptr(),
        path_c.as_ptr(),
        value_ptr,
      ))?;
    }

    let inner = NonNull::new(value_ptr).ok_or(Error::NullPointer)?;

    Ok(Value { inner, state: self })
  }

  /// Allocate a new value.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation fails.
  pub fn alloc_value(&self) -> Result<Value> {
    // SAFETY: context and state are valid
    let value_ptr = unsafe {
      sys::nix_alloc_value(self.context.as_ptr(), self.inner.as_ptr())
    };
    let inner = NonNull::new(value_ptr).ok_or(Error::NullPointer)?;

    Ok(Value { inner, state: self })
  }

  /// Get the raw state pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  unsafe fn as_ptr(&self) -> *mut sys::EvalState {
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
/// collections, and functions.
pub struct Value<'a> {
  inner: NonNull<sys::nix_value>,
  state: &'a EvalState,
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
      check_err(sys::nix_value_force(
        self.state.context.as_ptr(),
        self.state.as_ptr(),
        self.inner.as_ptr(),
      ))
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
      check_err(sys::nix_value_force_deep(
        self.state.context.as_ptr(),
        self.state.as_ptr(),
        self.inner.as_ptr(),
      ))
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

    // For string values, we need to use realised string API
    // SAFETY: context and value are valid, type is checked
    let realised_str = unsafe {
      sys::nix_string_realise(
        self.state.context.as_ptr(),
        self.state.as_ptr(),
        self.inner.as_ptr(),
        false, // don't copy more
      )
    };

    if realised_str.is_null() {
      return Err(Error::NullPointer);
    }

    // SAFETY: realised_str is non-null and points to valid RealizedString
    let buffer_start =
      unsafe { sys::nix_realised_string_get_buffer_start(realised_str) };

    let buffer_size =
      unsafe { sys::nix_realised_string_get_buffer_size(realised_str) };

    if buffer_start.is_null() {
      // Clean up realised string
      unsafe {
        sys::nix_realised_string_free(realised_str);
      }
      return Err(Error::NullPointer);
    }

    // SAFETY: buffer_start is non-null and buffer_size gives us the length
    let bytes = unsafe {
      std::slice::from_raw_parts(buffer_start.cast::<u8>(), buffer_size)
    };
    let string = std::str::from_utf8(bytes)
      .map_err(|_| Error::Unknown("Invalid UTF-8 in string".to_string()))?
      .to_owned();

    // Clean up realised string
    unsafe {
      sys::nix_realised_string_free(realised_str);
    }

    Ok(string)
  }

  /// Get the raw value pointer.
  ///
  /// # Safety
  ///
  /// The caller must ensure the pointer is used safely.
  #[allow(dead_code)]
  unsafe fn as_ptr(&self) -> *mut sys::nix_value {
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
      ValueType::Path => Ok("<path>".to_string()),
      ValueType::Thunk => Ok("<thunk>".to_string()),
      ValueType::External => Ok("<external>".to_string()),
    }
  }
}

impl Drop for Value<'_> {
  fn drop(&mut self) {
    // SAFETY: We own the value and it's valid until drop
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
      ValueType::Path => write!(f, "<path>"),
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
      ValueType::Path => write!(f, "Value::Path(<path>)"),
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
