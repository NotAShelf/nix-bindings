//! [`Value`]: Nix value wrapper with GC-managed lifetime. [`ValueType`]:
//! the discriminator enum.

#![cfg(feature = "expr")]

use std::{ffi::CStr, fmt, ptr::NonNull, sync::Arc};

use crate::{
  Error,
  EvalState,
  Result,
  error::check_err,
  store,
  sys,
  value_ops,
};

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
  pub(crate) fn from_c(value_type: sys::ValueType) -> Self {
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
      _ => ValueType::Thunk,
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

impl value_ops::NixValueRaw for Value<'_> {
  fn raw_ctx(&self) -> *mut sys::nix_c_context {
    // SAFETY: the wrapper holds the context alive via Arc.
    unsafe { self.state.context.as_ptr() }
  }

  fn raw_state(&self) -> *mut sys::EvalState {
    // SAFETY: the wrapper holds the state alive via &EvalState.
    unsafe { self.state.as_ptr() }
  }

  fn raw_inner(&self) -> *mut sys::nix_value {
    self.inner.as_ptr()
  }
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
    self.force_shared()
  }

  /// Force deep evaluation of this value.
  ///
  /// Forces evaluation of the value and all its nested components.
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
  ///
  /// Does **not** force; a lazy attribute or list element reports as
  /// [`ValueType::Thunk`] until forced. Use [`force`](Self::force) or any
  /// `as_*` accessor (which force implicitly) first.
  #[must_use]
  pub fn value_type(&self) -> ValueType {
    // SAFETY: context and value are valid
    let c_type = unsafe {
      sys::nix_get_type(self.state.context.as_ptr(), self.inner.as_ptr())
    };
    ValueType::from_c(c_type)
  }

  /// Get the Nix type name of this value as reported by the C API.
  ///
  /// Wraps `nix_get_typename`. Returns Nix's source-of-truth string (e.g.
  /// `"int"`, `"thunk"`, `"lambda"`). Falls back to the local
  /// [`ValueType::to_string`] if the C call returns null.
  #[must_use]
  pub fn type_name(&self) -> String {
    // SAFETY: context and value are valid.
    let ptr = unsafe {
      sys::nix_get_typename(self.state.context.as_ptr(), self.inner.as_ptr())
    };
    if ptr.is_null() {
      return self.value_type().to_string();
    }
    // SAFETY: ptr is non-null and points to a NUL-terminated C string.
    let s = unsafe { CStr::from_ptr(ptr) }
      .to_string_lossy()
      .into_owned();
    // nix_get_typename returns a strdup'd string; the caller owns it and
    // must free it.
    unsafe extern "C" {
      fn free(ptr: *mut std::os::raw::c_void);
    }
    // SAFETY: ptr came from strdup inside libnixexpr.
    unsafe { free(ptr.cast_mut().cast()) };
    s
  }

  /// Force this value via a shared reference.
  ///
  /// Used internally by the `as_*` accessors. The Nix C API mutates the
  /// underlying thunk on first force, but the operation is idempotent and
  /// the wrapper itself is not changed, so `&self` is sound.
  pub(crate) fn force_shared(&self) -> Result<()> {
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

  /// Convert this value to an integer. Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not an
  /// integer.
  pub fn as_int(&self) -> Result<i64> {
    self.force_shared()?;
    if self.value_type() != ValueType::Int {
      return Err(Error::InvalidType {
        expected: "int",
        actual:   self.value_type().to_string(),
      });
    }
    // SAFETY: context and value are valid, type is checked
    Ok(unsafe {
      sys::nix_get_int(self.state.context.as_ptr(), self.inner.as_ptr())
    })
  }

  /// Convert this value to a float. Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// float.
  pub fn as_float(&self) -> Result<f64> {
    self.force_shared()?;
    if self.value_type() != ValueType::Float {
      return Err(Error::InvalidType {
        expected: "float",
        actual:   self.value_type().to_string(),
      });
    }
    // SAFETY: context and value are valid, type is checked
    Ok(unsafe {
      sys::nix_get_float(self.state.context.as_ptr(), self.inner.as_ptr())
    })
  }

  /// Convert this value to a boolean. Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// boolean.
  pub fn as_bool(&self) -> Result<bool> {
    self.force_shared()?;
    if self.value_type() != ValueType::Bool {
      return Err(Error::InvalidType {
        expected: "bool",
        actual:   self.value_type().to_string(),
      });
    }
    // SAFETY: context and value are valid, type is checked
    Ok(unsafe {
      sys::nix_get_bool(self.state.context.as_ptr(), self.inner.as_ptr())
    })
  }

  /// Convert this value to a string.
  ///
  /// Realises any string context. Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the value is not a string.
  pub fn as_string(&self) -> Result<String> {
    self.force_shared()?;
    if self.value_type() != ValueType::String {
      return Err(Error::InvalidType {
        expected: "string",
        actual:   self.value_type().to_string(),
      });
    }

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
    let string = std::str::from_utf8(bytes)
      .map_err(|_| Error::Unknown("Invalid UTF-8 in string".to_string()))?
      .to_owned();

    unsafe { sys::nix_realised_string_free(realised_str) };

    Ok(string)
  }

  /// Convert this value to a string and return its store-path context.
  ///
  /// Extended form of [`as_string`](Self::as_string) returning both content
  /// and any store paths embedded in the string's context. For ordinary
  /// strings the context vector is empty.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not a string.
  pub fn as_string_with_context(
    &self,
  ) -> Result<(String, Vec<store::StorePath>)> {
    self.force_shared()?;
    if self.value_type() != ValueType::String {
      return Err(Error::InvalidType {
        expected: "string",
        actual:   self.value_type().to_string(),
      });
    }

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
    let string = match std::str::from_utf8(bytes) {
      Ok(s) => s.to_owned(),
      Err(_) => {
        unsafe { sys::nix_realised_string_free(realised_str) };
        return Err(Error::Unknown("Invalid UTF-8 in string".to_string()));
      },
    };

    let count =
      unsafe { sys::nix_realised_string_get_store_path_count(realised_str) };
    let mut paths = Vec::with_capacity(count);
    for i in 0..count {
      // SAFETY: index is in bounds
      let raw =
        unsafe { sys::nix_realised_string_get_store_path(realised_str, i) };
      if raw.is_null() {
        continue;
      }
      let cloned =
        unsafe { sys::nix_store_path_clone(raw as *mut sys::StorePath) };
      if let Some(inner) = NonNull::new(cloned) {
        paths.push(store::StorePath {
          inner,
          _context: Arc::clone(&self.state.context),
        });
      }
    }

    unsafe { sys::nix_realised_string_free(realised_str) };

    Ok((string, paths))
  }

  /// Convert this value to a filesystem path. Forces the value first.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the value is not a path.
  pub fn as_path(&self) -> Result<std::path::PathBuf> {
    self.force_shared()?;
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

    // On Unix the kernel treats paths as arbitrary bytes; round-trip
    // through OsStr to preserve any non-UTF-8 bytes.
    #[cfg(unix)]
    {
      use std::os::unix::ffi::OsStrExt as _;
      let bytes = unsafe { CStr::from_ptr(path_ptr) }.to_bytes();
      Ok(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
      let path_str =
        unsafe { CStr::from_ptr(path_ptr).to_string_lossy().into_owned() };
      Ok(std::path::PathBuf::from(path_str))
    }
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
  /// Unlike [`call`](Self::call), this does not perform the call
  /// immediately; it stores it as a thunk. Useful for lazy attribute sets
  /// and lists.
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

  /// Copy this value into a new owned value slot.
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

  /// Format this value as Nix syntax.
  ///
  /// Forces the value first and recursively renders attribute sets and
  /// lists. Functions, thunks, and external values render as opaque
  /// placeholders (`<lambda>`, `<thunk>`, `<external>`) the same way
  /// `nix-instantiate --eval` does.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or any nested value cannot be
  /// rendered.
  pub fn to_nix_string(&self) -> Result<String> {
    self.force_shared()?;
    match self.value_type() {
      ValueType::Int => Ok(self.as_int()?.to_string()),
      ValueType::Float => Ok(self.as_float()?.to_string()),
      ValueType::Bool => {
        Ok(if self.as_bool()? { "true" } else { "false" }.to_string())
      },
      ValueType::String => {
        let s = self.as_string()?;
        Ok(format!(
          "\"{}\"",
          s.replace('\\', "\\\\").replace('"', "\\\"")
        ))
      },
      ValueType::Null => Ok("null".to_string()),
      ValueType::Path => {
        Ok(
          self
            .as_path()
            .map_or_else(|_| "<path>".to_string(), |p| p.display().to_string()),
        )
      },
      ValueType::Attrs => {
        let mut parts = Vec::new();
        for entry in self.attrs()? {
          let (k, v) = entry?;
          parts.push(format!("{k} = {};", v.to_nix_string()?));
        }
        if parts.is_empty() {
          Ok("{ }".to_string())
        } else {
          Ok(format!("{{ {} }}", parts.join(" ")))
        }
      },
      ValueType::List => {
        let mut parts = Vec::new();
        for entry in self.list_iter()? {
          parts.push(entry?.to_nix_string()?);
        }
        if parts.is_empty() {
          Ok("[ ]".to_string())
        } else {
          Ok(format!("[ {} ]", parts.join(" ")))
        }
      },
      ValueType::Function => Ok("<lambda>".to_string()),
      ValueType::Thunk => Ok("<thunk>".to_string()),
      ValueType::External => Ok("<external>".to_string()),
    }
  }
}

impl Drop for Value<'_> {
  fn drop(&mut self) {
    // SAFETY: We hold a GC reference; release it.
    unsafe {
      sys::nix_value_decref(self.state.context.as_ptr(), self.inner.as_ptr());
    }
  }
}

impl<'a> Clone for Value<'a> {
  /// Clone a [`Value`] by incrementing its GC reference count.
  ///
  /// Both clones reference the same underlying Nix value; they are not
  /// deep copies. Use [`copy`](Value::copy) when you need an independent
  /// value slot.
  fn clone(&self) -> Self {
    // SAFETY: context and value are valid; incref is idempotent.
    let err = unsafe {
      sys::nix_value_incref(self.state.context.as_ptr(), self.inner.as_ptr())
    };
    assert!(
      err == sys::nix_err_NIX_OK,
      "nix_value_incref failed with code {err}"
    );
    Value {
      inner: self.inner,
      state: self.state,
    }
  }
}

impl fmt::Display for Value<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.value_type() {
      ValueType::Int => {
        match self.as_int() {
          Ok(v) => write!(f, "{v}"),
          Err(_) => write!(f, "<int error>"),
        }
      },
      ValueType::Float => {
        match self.as_float() {
          Ok(v) => write!(f, "{v}"),
          Err(_) => write!(f, "<float error>"),
        }
      },
      ValueType::Bool => {
        match self.as_bool() {
          Ok(v) => write!(f, "{v}"),
          Err(_) => write!(f, "<bool error>"),
        }
      },
      ValueType::String => {
        match self.as_string() {
          Ok(v) => write!(f, "{v}"),
          Err(_) => write!(f, "<string error>"),
        }
      },
      ValueType::Null => write!(f, "null"),
      ValueType::Attrs => write!(f, "{{ <attrs> }}"),
      ValueType::List => write!(f, "[ <list> ]"),
      ValueType::Function => write!(f, "<function>"),
      ValueType::Path => {
        match self.as_path() {
          Ok(p) => write!(f, "{}", p.display()),
          Err(_) => write!(f, "<path>"),
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
        match self.as_int() {
          Ok(v) => write!(f, "Value::Int({v})"),
          Err(_) => write!(f, "Value::Int(<error>)"),
        }
      },
      ValueType::Float => {
        match self.as_float() {
          Ok(v) => write!(f, "Value::Float({v})"),
          Err(_) => write!(f, "Value::Float(<error>)"),
        }
      },
      ValueType::Bool => {
        match self.as_bool() {
          Ok(v) => write!(f, "Value::Bool({v})"),
          Err(_) => write!(f, "Value::Bool(<error>)"),
        }
      },
      ValueType::String => {
        match self.as_string() {
          Ok(v) => write!(f, "Value::String({v:?})"),
          Err(_) => write!(f, "Value::String(<error>)"),
        }
      },
      ValueType::Null => write!(f, "Value::Null"),
      ValueType::Attrs => write!(f, "Value::Attrs({{ <attrs> }})"),
      ValueType::List => write!(f, "Value::List([ <list> ])"),
      ValueType::Function => write!(f, "Value::Function(<function>)"),
      ValueType::Path => {
        match self.as_path() {
          Ok(p) => write!(f, "Value::Path({})", p.display()),
          Err(_) => write!(f, "Value::Path(<path>)"),
        }
      },
      ValueType::Thunk => write!(f, "Value::Thunk(<thunk>)"),
      ValueType::External => write!(f, "Value::External(<external>)"),
    }
  }
}
