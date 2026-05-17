//! Nix primitive operations (primops).
//!
//! This module provides a safe, closure-based API for registering custom Nix
//! primitive operations (primops).
//!
//! # Overview
//!
//! Primops are Rust functions that appear as Nix builtins. There are two ways
//! to expose a primop:
//!
//! * **Global builtin**: call [`PrimOp::register`] *before* creating any
//!   [`EvalState`](crate::EvalState). All subsequently created states will
//!   include the primop in `builtins`.
//! * **Value-embedded**: call [`PrimOp::into_value`] on an existing
//!   [`EvalState`](crate::EvalState) to obtain a callable
//!   [`Value`](crate::Value).
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use nix_bindings::{Context, EvalStateBuilder, Store, primop::PrimOp};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let ctx = Arc::new(Context::new()?);
//!
//! // Register a global builtin that doubles an integer
//! PrimOp::new(&ctx, "double", 1, Some("Double an integer"), |args, ret| {
//!   let n = args[0].as_int()?;
//!   ret.set_int(n * 2)
//! })?
//! .register(&ctx)?;
//!
//! let store = Arc::new(Store::open(&ctx, None)?);
//! let state = EvalStateBuilder::new(&store)?.build()?;
//! let result = state.eval_from_string("builtins.double 21", "<eval>")?;
//! assert_eq!(result.as_int()?, 42);
//! # Ok(())
//! # }
//! ```

use std::{
  ffi::CString,
  marker::PhantomData,
  os::raw::c_void,
  panic::{self, AssertUnwindSafe},
  sync::Arc,
};

use crate::{Context, Error, Result, ValueType, check_err, sys};

type PrimOpFn =
  dyn Fn(&[PrimOpArg<'_>], &mut PrimOpRet<'_>) -> Result<()> + Send + Sync;

/// The boxed Rust closure called by the trampoline.
///
/// Arity is stored alongside the closure so the trampoline can slice the args
/// array correctly.
struct ClosureData {
  arity: usize,
  f:     Box<PrimOpFn>,
}

/// C-compatible trampoline that dispatches to the boxed Rust closure.
///
/// # Safety
///
/// `user_data` must be a valid `*mut ClosureData` allocated via
/// `Box::into_raw`.  `args` must be a valid array of at least `arity`
/// non-null `*mut nix_value` pointers. `ret` must be a valid, writable
/// `*mut nix_value`.
unsafe extern "C" fn trampoline(
  user_data: *mut c_void,
  context: *mut sys::nix_c_context,
  state: *mut sys::EvalState,
  args: *mut *mut sys::nix_value,
  ret: *mut sys::nix_value,
) {
  let data = unsafe { &*(user_data as *const ClosureData) };

  // Build arg wrappers.
  //
  // The pointers are borrowed from Nix and must NOT be
  // decreffed by our code. When arity is 0, `args` may be null. Passing a null
  // pointer to slice::from_raw_parts (even with len=0) produces a dangling
  // reference which violates Rust's validity invariant for references.
  let arg_wrappers: Vec<PrimOpArg<'_>> = if data.arity == 0 {
    Vec::new()
  } else {
    let arg_slice = unsafe { std::slice::from_raw_parts(args, data.arity) };
    arg_slice
      .iter()
      .map(|&p| {
        PrimOpArg {
          inner: p,
          ctx: context,
          state,
          _phantom: PhantomData,
        }
      })
      .collect()
  };

  let mut ret_wrapper = PrimOpRet {
    inner: ret,
    ctx: context,
    state,
    _phantom: PhantomData,
  };

  let result = panic::catch_unwind(AssertUnwindSafe(|| {
    (data.f)(&arg_wrappers, &mut ret_wrapper)
  }));

  let err_msg = match result {
    Ok(Ok(())) => return,
    Ok(Err(e)) => format!("primop error: {e}"),
    Err(_) => "primop panicked".to_string(),
  };

  let msg_c = CString::new(err_msg)
    .unwrap_or_else(|_| CString::new("primop error").unwrap());
  unsafe {
    sys::nix_set_err_msg(
      context,
      sys::nix_err_NIX_ERR_NIX_ERROR,
      msg_c.as_ptr(),
    );
  }
}

/// GC finalizer that frees the `ClosureData` box when the GC collects the
/// associated `PrimOp` object.
///
/// `cd` is the `*mut ClosureData` stored at primop-allocation time.
unsafe extern "C" fn drop_closure_finalizer(
  _obj: *mut c_void,
  cd: *mut c_void,
) {
  // Reconstruct and immediately drop the Box, running the destructor.
  let _ = unsafe { Box::from_raw(cd as *mut ClosureData) };
}

/// A borrowed Nix value passed as an argument to a primop callback.
///
/// The value is owned by the Nix evaluator and must **not** be decreffed by
/// the caller. It is valid only for the duration of the primop invocation
/// (expressed by the `'a` lifetime).
pub struct PrimOpArg<'a> {
  inner:    *mut sys::nix_value,
  ctx:      *mut sys::nix_c_context,
  state:    *mut sys::EvalState,
  _phantom: PhantomData<&'a ()>,
}

// PrimOpArg is only used within the synchronous callback; no cross-thread use.
unsafe impl Send for PrimOpArg<'_> {}
unsafe impl Sync for PrimOpArg<'_> {}

impl PrimOpArg<'_> {
  /// Return the [`ValueType`] of this argument.
  #[must_use]
  pub fn value_type(&self) -> ValueType {
    let c_type = unsafe { sys::nix_get_type(self.ctx, self.inner) };
    ValueType::from_c(c_type)
  }

  /// Force evaluation of this argument (resolves thunks).
  ///
  /// # Errors
  ///
  /// Returns an error if evaluation fails.
  pub fn force(&self) -> Result<()> {
    unsafe {
      check_err(
        self.ctx,
        sys::nix_value_force(self.ctx, self.state, self.inner),
      )
    }
  }

  /// Extract this argument as an integer.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not an
  /// integer.
  pub fn as_int(&self) -> Result<i64> {
    self.force()?;
    if self.value_type() != ValueType::Int {
      return Err(Error::InvalidType {
        expected: "int",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(unsafe { sys::nix_get_int(self.ctx, self.inner) })
  }

  /// Extract this argument as a float.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// float.
  pub fn as_float(&self) -> Result<f64> {
    self.force()?;
    if self.value_type() != ValueType::Float {
      return Err(Error::InvalidType {
        expected: "float",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(unsafe { sys::nix_get_float(self.ctx, self.inner) })
  }

  /// Extract this argument as a boolean.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// boolean.
  pub fn as_bool(&self) -> Result<bool> {
    self.force()?;
    if self.value_type() != ValueType::Bool {
      return Err(Error::InvalidType {
        expected: "bool",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(unsafe { sys::nix_get_bool(self.ctx, self.inner) })
  }

  /// Extract this argument as a UTF-8 string.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails, the resolved value is not a
  /// string, or the string contains invalid UTF-8.
  pub fn as_string(&self) -> Result<String> {
    self.force()?;
    if self.value_type() != ValueType::String {
      return Err(Error::InvalidType {
        expected: "string",
        actual:   self.value_type().to_string(),
      });
    }

    let realised_str = unsafe {
      sys::nix_string_realise(self.ctx, self.state, self.inner, false)
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

  /// Interpret this argument as an attribute set.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// Returns an [`ArgAttrs`] wrapper that provides read access to the
  /// attribute set's keys and values. The returned wrapper borrows this
  /// argument; it does not own any GC references.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not an
  /// attribute set.
  pub fn as_attrs(&self) -> Result<ArgAttrs<'_>> {
    self.force()?;
    if self.value_type() != ValueType::Attrs {
      return Err(Error::InvalidType {
        expected: "attrs",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(ArgAttrs {
      inner:    self.inner,
      ctx:      self.ctx,
      state:    self.state,
      _phantom: PhantomData,
    })
  }

  /// Interpret this argument as a list.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// Returns an [`ArgList`] wrapper that provides read access to the list
  /// elements. The returned wrapper borrows this argument; it does not own
  /// any GC references.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// list.
  pub fn as_list(&self) -> Result<ArgList<'_>> {
    self.force()?;
    if self.value_type() != ValueType::List {
      return Err(Error::InvalidType {
        expected: "list",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(ArgList {
      inner:    self.inner,
      ctx:      self.ctx,
      state:    self.state,
      _phantom: PhantomData,
    })
  }
}

/// The writable return-value slot provided to a primop closure.
///
/// Exactly one `set_*` method must be called before returning `Ok(())`.
pub struct PrimOpRet<'a> {
  inner:    *mut sys::nix_value,
  ctx:      *mut sys::nix_c_context,
  state:    *mut sys::EvalState,
  _phantom: PhantomData<&'a mut ()>,
}

impl PrimOpRet<'_> {
  /// Write an integer result.
  ///
  /// # Errors
  ///
  /// Returns an error if the write fails.
  pub fn set_int(&mut self, i: i64) -> Result<()> {
    unsafe { check_err(self.ctx, sys::nix_init_int(self.ctx, self.inner, i)) }
  }

  /// Write a float result.
  ///
  /// # Errors
  ///
  /// Returns an error if the write fails.
  pub fn set_float(&mut self, f: f64) -> Result<()> {
    unsafe { check_err(self.ctx, sys::nix_init_float(self.ctx, self.inner, f)) }
  }

  /// Write a boolean result.
  ///
  /// # Errors
  ///
  /// Returns an error if the write fails.
  pub fn set_bool(&mut self, b: bool) -> Result<()> {
    unsafe { check_err(self.ctx, sys::nix_init_bool(self.ctx, self.inner, b)) }
  }

  /// Write a null result.
  ///
  /// # Errors
  ///
  /// Returns an error if the write fails.
  pub fn set_null(&mut self) -> Result<()> {
    unsafe { check_err(self.ctx, sys::nix_init_null(self.ctx, self.inner)) }
  }

  /// Write a string result.
  ///
  /// # Errors
  ///
  /// Returns an error if `s` contains an interior NUL byte or the write
  /// fails.
  pub fn set_string(&mut self, s: &str) -> Result<()> {
    let s_c = CString::new(s)?;
    unsafe {
      check_err(
        self.ctx,
        sys::nix_init_string(self.ctx, self.inner, s_c.as_ptr()),
      )
    }
  }

  /// Copy the value pointed to by `src` into the return slot.
  ///
  /// This is useful when the result is an existing [`Value`](crate::Value)
  /// that should be forwarded as-is.
  ///
  /// # Safety
  ///
  /// `src` must be a valid, non-null `*mut nix_value` that remains live for
  /// the duration of the call.
  ///
  /// # Errors
  ///
  /// Returns an error if the copy fails.
  pub unsafe fn copy_from_raw(
    &mut self,
    src: *mut sys::nix_value,
  ) -> Result<()> {
    unsafe {
      check_err(self.ctx, sys::nix_copy_value(self.ctx, self.inner, src))
    }
  }

  /// Write an attribute set result.
  ///
  /// Builds an attribute set from the given key-value pairs and writes it
  /// into the return slot. Each value is a [`PrimOpValue`] obtained from a
  /// primop argument or created via the `make_*` methods on this struct.
  ///
  /// # Errors
  ///
  /// Returns an error if construction fails.
  pub fn set_attrs(&mut self, pairs: &[(&str, &PrimOpValue)]) -> Result<()> {
    let builder = unsafe {
      sys::nix_make_bindings_builder(self.ctx, self.state, pairs.len().max(1))
    };
    if builder.is_null() {
      return Err(Error::NullPointer);
    }

    struct BuilderGuard(*mut sys::BindingsBuilder);
    impl Drop for BuilderGuard {
      fn drop(&mut self) {
        unsafe { sys::nix_bindings_builder_free(self.0) };
      }
    }
    let _guard = BuilderGuard(builder);

    for (key, value) in pairs {
      let key_c = CString::new(*key)?;
      // SAFETY: builder, key, and value are valid
      unsafe {
        check_err(
          self.ctx,
          sys::nix_bindings_builder_insert(
            self.ctx,
            builder,
            key_c.as_ptr(),
            value.inner,
          ),
        )?;
      }
    }

    // SAFETY: builder is valid, result slot is valid
    unsafe {
      check_err(self.ctx, sys::nix_make_attrs(self.ctx, self.inner, builder))
    }
  }

  /// Write a list result.
  ///
  /// Builds a list from the given values and writes it into the return
  /// slot. Each value is a [`PrimOpValue`] obtained from a primop
  /// argument or created via the `make_*` methods on this struct.
  ///
  /// # Errors
  ///
  /// Returns an error if construction fails.
  pub fn set_list(&mut self, items: &[&PrimOpValue]) -> Result<()> {
    let builder = unsafe {
      sys::nix_make_list_builder(self.ctx, self.state, items.len().max(1))
    };
    if builder.is_null() {
      return Err(Error::NullPointer);
    }

    struct ListGuard(*mut sys::ListBuilder);
    impl Drop for ListGuard {
      fn drop(&mut self) {
        unsafe { sys::nix_list_builder_free(self.0) };
      }
    }
    let _guard = ListGuard(builder);

    for (i, value) in items.iter().enumerate() {
      unsafe {
        check_err(
          self.ctx,
          sys::nix_list_builder_insert(
            self.ctx,
            builder,
            i as std::os::raw::c_uint,
            value.inner,
          ),
        )?;
      }
    }

    unsafe {
      check_err(self.ctx, sys::nix_make_list(self.ctx, builder, self.inner))
    }
  }

  /// Allocate and initialise an integer [`PrimOpValue`].
  ///
  /// # Errors
  ///
  /// Returns an error if allocation or initialisation fails.
  pub fn make_int(&self, i: i64) -> Result<PrimOpValue> {
    let v = PrimOpValue::alloc(self.ctx, self.state)?;
    unsafe {
      check_err(self.ctx, sys::nix_init_int(self.ctx, v.inner, i))?;
    }
    Ok(v)
  }

  /// Allocate and initialise a float [`PrimOpValue`].
  ///
  /// # Errors
  ///
  /// Returns an error if allocation or initialisation fails.
  pub fn make_float(&self, f: f64) -> Result<PrimOpValue> {
    let v = PrimOpValue::alloc(self.ctx, self.state)?;
    unsafe {
      check_err(self.ctx, sys::nix_init_float(self.ctx, v.inner, f))?;
    }
    Ok(v)
  }

  /// Allocate and initialise a boolean [`PrimOpValue`].
  ///
  /// # Errors
  ///
  /// Returns an error if allocation or initialisation fails.
  pub fn make_bool(&self, b: bool) -> Result<PrimOpValue> {
    let v = PrimOpValue::alloc(self.ctx, self.state)?;
    unsafe {
      check_err(self.ctx, sys::nix_init_bool(self.ctx, v.inner, b))?;
    }
    Ok(v)
  }

  /// Allocate and initialise a null [`PrimOpValue`].
  ///
  /// # Errors
  ///
  /// Returns an error if allocation or initialisation fails.
  pub fn make_null(&self) -> Result<PrimOpValue> {
    let v = PrimOpValue::alloc(self.ctx, self.state)?;
    unsafe {
      check_err(self.ctx, sys::nix_init_null(self.ctx, v.inner))?;
    }
    Ok(v)
  }

  /// Allocate and initialise a string [`PrimOpValue`].
  ///
  /// # Errors
  ///
  /// Returns an error if allocation, string conversion, or initialisation
  /// fails.
  pub fn make_string(&self, s: &str) -> Result<PrimOpValue> {
    let v = PrimOpValue::alloc(self.ctx, self.state)?;
    let s_c = CString::new(s)?;
    unsafe {
      check_err(
        self.ctx,
        sys::nix_init_string(self.ctx, v.inner, s_c.as_ptr()),
      )?;
    }
    Ok(v)
  }
}

/// An owned Nix value used within a primop callback.
///
/// Unlike [`PrimOpArg`], this owns a GC reference to the underlying value
/// and decrements it on drop. It is returned by [`ArgAttrs::get`] and
/// [`ArgList::get`] when extracting child values from collections.
///
/// Methods mirror those of [`PrimOpArg`]: `value_type`, `force`, `as_int`,
/// `as_float`, `as_bool`, `as_string`, `as_attrs`, and `as_list` are all
/// available.
pub struct PrimOpValue {
  inner: *mut sys::nix_value,
  ctx:   *mut sys::nix_c_context,
  state: *mut sys::EvalState,
}

unsafe impl Send for PrimOpValue {}
unsafe impl Sync for PrimOpValue {}

impl PrimOpValue {
  fn alloc(
    ctx: *mut sys::nix_c_context,
    state: *mut sys::EvalState,
  ) -> Result<Self> {
    let inner = unsafe { sys::nix_alloc_value(ctx, state) };
    if inner.is_null() {
      return Err(Error::NullPointer);
    }
    Ok(PrimOpValue { inner, ctx, state })
  }

  /// Return the [`ValueType`] of this value.
  #[must_use]
  pub fn value_type(&self) -> ValueType {
    let c_type = unsafe { sys::nix_get_type(self.ctx, self.inner) };
    ValueType::from_c(c_type)
  }

  /// Force evaluation of this value (resolves thunks).
  ///
  /// # Errors
  ///
  /// Returns an error if evaluation fails.
  pub fn force(&self) -> Result<()> {
    unsafe {
      check_err(
        self.ctx,
        sys::nix_value_force(self.ctx, self.state, self.inner),
      )
    }
  }

  /// Extract this value as an integer.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not an
  /// integer.
  pub fn as_int(&self) -> Result<i64> {
    self.force()?;
    if self.value_type() != ValueType::Int {
      return Err(Error::InvalidType {
        expected: "int",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(unsafe { sys::nix_get_int(self.ctx, self.inner) })
  }

  /// Extract this value as a float.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// float.
  pub fn as_float(&self) -> Result<f64> {
    self.force()?;
    if self.value_type() != ValueType::Float {
      return Err(Error::InvalidType {
        expected: "float",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(unsafe { sys::nix_get_float(self.ctx, self.inner) })
  }

  /// Extract this value as a boolean.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// boolean.
  pub fn as_bool(&self) -> Result<bool> {
    self.force()?;
    if self.value_type() != ValueType::Bool {
      return Err(Error::InvalidType {
        expected: "bool",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(unsafe { sys::nix_get_bool(self.ctx, self.inner) })
  }

  /// Extract this value as a UTF-8 string.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails, the resolved value is not a
  /// string, or the string contains invalid UTF-8.
  pub fn as_string(&self) -> Result<String> {
    self.force()?;
    if self.value_type() != ValueType::String {
      return Err(Error::InvalidType {
        expected: "string",
        actual:   self.value_type().to_string(),
      });
    }

    let realised_str = unsafe {
      sys::nix_string_realise(self.ctx, self.state, self.inner, false)
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

  /// Interpret this value as an attribute set.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// Returns an [`ArgAttrs`] wrapper that borrows from this value.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not an
  /// attribute set.
  #[must_use]
  pub fn as_attrs(&self) -> Result<ArgAttrs<'_>> {
    self.force()?;
    if self.value_type() != ValueType::Attrs {
      return Err(Error::InvalidType {
        expected: "attrs",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(ArgAttrs {
      inner:    self.inner,
      ctx:      self.ctx,
      state:    self.state,
      _phantom: PhantomData,
    })
  }

  /// Interpret this value as a list.
  ///
  /// Automatically forces the value if it is a thunk.
  ///
  /// Returns an [`ArgList`] wrapper that borrows from this value.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing fails or the resolved value is not a
  /// list.
  #[must_use]
  pub fn as_list(&self) -> Result<ArgList<'_>> {
    self.force()?;
    if self.value_type() != ValueType::List {
      return Err(Error::InvalidType {
        expected: "list",
        actual:   self.value_type().to_string(),
      });
    }
    Ok(ArgList {
      inner:    self.inner,
      ctx:      self.ctx,
      state:    self.state,
      _phantom: PhantomData,
    })
  }
}

impl Drop for PrimOpValue {
  fn drop(&mut self) {
    // SAFETY: ctx and inner are valid; we own a GC reference
    unsafe {
      sys::nix_value_decref(self.ctx, self.inner);
    }
  }
}
/// A borrowed Nix attribute set value.
///
/// Obtained from [`PrimOpArg::as_attrs`] or [`PrimOpValue::as_attrs`].
/// Provides read access to the attribute set without owning any GC
/// references itself.
pub struct ArgAttrs<'a> {
  inner:    *mut sys::nix_value,
  ctx:      *mut sys::nix_c_context,
  state:    *mut sys::EvalState,
  _phantom: PhantomData<&'a ()>,
}

unsafe impl Send for ArgAttrs<'_> {}
unsafe impl Sync for ArgAttrs<'_> {}

impl ArgAttrs<'_> {
  /// Get an attribute by name.
  ///
  /// Returns an owned [`PrimOpValue`] that holds a GC reference to the
  /// attribute's value. The caller is responsible for the GC reference;
  /// [`PrimOpValue`] releases it on drop.
  ///
  /// # Errors
  ///
  /// Returns [`Error::KeyNotFound`] if the key does not exist.
  pub fn get(&self, key: &str) -> Result<PrimOpValue> {
    let key_c = CString::new(key)?;
    // SAFETY: ctx, value, and state are valid
    let ptr = unsafe {
      sys::nix_get_attr_byname(self.ctx, self.inner, self.state, key_c.as_ptr())
    };
    if ptr.is_null() {
      return Err(Error::KeyNotFound(key.to_string()));
    }
    Ok(PrimOpValue {
      inner: ptr,
      ctx:   self.ctx,
      state: self.state,
    })
  }

  /// Check if an attribute exists.
  ///
  /// # Errors
  ///
  /// Returns an error if the lookup itself fails (not if the key is
  /// absent).
  pub fn has(&self, key: &str) -> Result<bool> {
    let key_c = CString::new(key)?;
    // SAFETY: ctx, value, and state are valid
    let result = unsafe {
      sys::nix_has_attr_byname(self.ctx, self.inner, self.state, key_c.as_ptr())
    };
    Ok(result)
  }

  /// Return all attribute keys in this set.
  ///
  /// # Errors
  ///
  /// Returns an error if iteration fails.
  pub fn keys(&self) -> Result<Vec<String>> {
    // SAFETY: ctx and value are valid
    let count = unsafe { sys::nix_get_attrs_size(self.ctx, self.inner) };

    let mut keys = Vec::with_capacity(count as usize);
    for i in 0..count {
      let mut name_ptr: *const std::os::raw::c_char = std::ptr::null();
      let val_ptr = unsafe {
        sys::nix_get_attr_byidx(
          self.ctx,
          self.inner,
          self.state,
          i,
          &mut name_ptr,
        )
      };
      // We only want the name; release the value reference.
      if !val_ptr.is_null() {
        unsafe { sys::nix_value_decref(self.ctx, val_ptr) };
      }
      if name_ptr.is_null() {
        continue;
      }
      let name = unsafe {
        std::ffi::CStr::from_ptr(name_ptr)
          .to_string_lossy()
          .into_owned()
      };
      keys.push(name);
    }
    Ok(keys)
  }

  /// Return the number of attributes in this set.
  #[must_use]
  pub fn len(&self) -> usize {
    unsafe { sys::nix_get_attrs_size(self.ctx, self.inner) as usize }
  }

  /// Return `true` if the attribute set is empty.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }
}

/// A borrowed Nix list value.
///
/// Obtained from [`PrimOpArg::as_list`] or [`PrimOpValue::as_list`].
/// Provides read access to the list without owning any GC references
/// itself.
pub struct ArgList<'a> {
  inner:    *mut sys::nix_value,
  ctx:      *mut sys::nix_c_context,
  state:    *mut sys::EvalState,
  _phantom: PhantomData<&'a ()>,
}

unsafe impl Send for ArgList<'_> {}
unsafe impl Sync for ArgList<'_> {}

impl ArgList<'_> {
  /// Get an element by index.
  ///
  /// Returns an owned [`PrimOpValue`] that holds a GC reference to the
  /// element. The caller is responsible for the GC reference;
  /// [`PrimOpValue`] releases it on drop.
  ///
  /// # Errors
  ///
  /// Returns [`Error::IndexOutOfBounds`] if `index >= self.len()`.
  pub fn get(&self, index: usize) -> Result<PrimOpValue> {
    let length = self.len();
    if index >= length {
      return Err(Error::IndexOutOfBounds { index, length });
    }
    // SAFETY: ctx, value, and state are valid; index is bounds-checked
    let ptr = unsafe {
      sys::nix_get_list_byidx(
        self.ctx,
        self.inner,
        self.state,
        index as std::os::raw::c_uint,
      )
    };
    if ptr.is_null() {
      return Err(Error::NullPointer);
    }
    Ok(PrimOpValue {
      inner: ptr,
      ctx:   self.ctx,
      state: self.state,
    })
  }

  /// Return the number of elements in this list.
  #[must_use]
  pub fn len(&self) -> usize {
    unsafe { sys::nix_get_list_size(self.ctx, self.inner) as usize }
  }

  /// Return `true` if the list is empty.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }
}

/// A Nix primitive operation (primop) wrapping a Rust closure.
///
/// After construction with [`PrimOp::new`], the primop can be:
///
/// * Registered as a **global builtin** via [`PrimOp::register`] (must happen
///   before creating any [`EvalState`](crate::EvalState)).
/// * Embedded in a **value** via [`PrimOp::into_value`] for use as a
///   first-class function inside an [`EvalState`](crate::EvalState).
pub struct PrimOp {
  /// GC-owned pointer; we hold one reference until consumed.
  inner:      *mut sys::PrimOp,
  /// Keeps the context alive for the lifetime of the PrimOp.
  context:    Arc<Context>,
  /// Set to `true` after `register()` so Drop skips the decref.
  registered: bool,
}

// SAFETY: PrimOp contains raw pointers but is only ever manipulated on a
// single thread. The underlying PrimOp object is GC-managed.
unsafe impl Send for PrimOp {}
unsafe impl Sync for PrimOp {}

impl PrimOp {
  /// Create a new primop backed by the given Rust closure.
  ///
  /// # Arguments
  ///
  /// * `context`: the Nix context.
  /// * `name`: the name of the primop as it will appear in Nix.
  /// * `arity`: number of arguments the primop accepts.
  /// * `doc`: optional documentation string.
  /// * `f`: the Rust closure to invoke.
  ///
  /// # Errors
  ///
  /// Returns an error if the name or doc string contains an interior NUL
  /// byte, or if the underlying allocation fails.
  pub fn new<F>(
    context: &Arc<Context>,
    name: &str,
    arity: u32,
    doc: Option<&str>,
    f: F,
  ) -> Result<Self>
  where
    F: Fn(&[PrimOpArg<'_>], &mut PrimOpRet<'_>) -> Result<()>
      + Send
      + Sync
      + 'static,
  {
    let name_c = CString::new(name)?;
    let doc_c = doc.map(CString::new).transpose()?;
    // PrimOp::doc is std::optional<std::string> since Nix 2.34.
    // Passing null would throw "construction from null".
    let empty_doc;
    let doc_ptr = match doc_c {
      Some(ref c) => c.as_ptr(),
      None => {
        empty_doc = CString::default();
        empty_doc.as_ptr()
      },
    };

    // Box the closure together with its arity for the trampoline.
    let data = Box::new(ClosureData {
      arity: arity as usize,
      f:     Box::new(f),
    });
    let data_raw = Box::into_raw(data) as *mut c_void;

    // Allocate the GC-managed PrimOp.
    // SAFETY: context is valid; trampoline has the expected C signature.
    let primop_ptr = unsafe {
      sys::nix_alloc_primop(
        context.as_ptr(),
        Some(trampoline),
        arity as std::os::raw::c_int,
        name_c.as_ptr(),
        std::ptr::null_mut(), // arg names (optional)
        doc_ptr,
        data_raw,
      )
    };

    if primop_ptr.is_null() {
      let _ = unsafe { Box::from_raw(data_raw as *mut ClosureData) };
      // SAFETY: context pointer is valid
      unsafe {
        check_err(context.as_ptr(), sys::nix_err_code(context.as_ptr()))?;
      }
      return Err(Error::NullPointer);
    }

    // Register a GC finalizer so the closure is freed when the PrimOp GC
    // object is collected.
    // SAFETY: primop_ptr is a valid GC object; data_raw is a valid pointer.
    unsafe {
      sys::nix_gc_register_finalizer(
        primop_ptr as *mut c_void,
        data_raw,
        Some(drop_closure_finalizer),
      );
    }

    Ok(PrimOp {
      inner:      primop_ptr,
      context:    Arc::clone(context),
      registered: false,
    })
  }

  /// Register this primop as a global Nix builtin.
  ///
  /// After this call the primop will appear in `builtins.*` for all
  /// [`EvalState`](crate::EvalState) instances created **after** this call.
  ///
  /// This consumes `self`; the underlying pointer is transferred to the
  /// global registry and is no longer accessible.
  ///
  /// # Errors
  ///
  /// Returns an error if the registration fails.
  pub fn register(mut self, context: &Context) -> Result<()> {
    // SAFETY: context and inner are valid
    let err = unsafe { sys::nix_register_primop(context.as_ptr(), self.inner) };
    check_err(unsafe { self.context.as_ptr() }, err)?;
    // Mark as registered only after confirmed success so Drop still calls
    // nix_gc_decref if registration fails.
    self.registered = true;
    Ok(())
  }

  /// Embed this primop in a Nix value, returning a callable
  /// [`Value`](crate::Value).
  ///
  /// This consumes `self`.  The returned value holds a GC reference to the
  /// primop, keeping it alive for the lifetime of the value.
  ///
  /// # Errors
  ///
  /// Returns an error if the value allocation or initialisation fails.
  pub fn into_value(
    self,
    state: &crate::EvalState,
  ) -> Result<crate::Value<'_>> {
    let v = state.alloc_value()?;
    // SAFETY: context, value, and primop pointer are valid
    unsafe {
      check_err(
        state.context.as_ptr(),
        sys::nix_init_primop(
          state.context.as_ptr(),
          v.inner.as_ptr(),
          self.inner,
        ),
      )?;
    }
    // `self` drops here; the value holds the GC ref via nix_init_primop so
    // our own reference can be released.
    Ok(v)
  }
}

impl Drop for PrimOp {
  fn drop(&mut self) {
    if !self.registered && !self.inner.is_null() {
      // Release our GC reference.  The GC may still keep the object alive
      // until it is collected, at which point the finalizer frees the
      // closure.
      // SAFETY: ctx and inner are valid
      unsafe {
        let _ = sys::nix_gc_decref(
          self.context.as_ptr(),
          self.inner as *const c_void,
        );
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use serial_test::serial;

  use super::*;
  use crate::{Context, EvalStateBuilder, Store};

  #[test]
  #[serial]
  fn test_primop_into_value() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    // A primop that negates an integer
    let primop =
      PrimOp::new(&ctx, "negate", 1, Some("Negate an integer"), |args, ret| {
        let n = args[0].as_int()?;
        ret.set_int(-n)
      })
      .expect("Failed to create primop");

    let func = primop
      .into_value(&state)
      .expect("Failed to embed primop as value");

    let arg = state.make_int(7).unwrap();
    let result = func.call(&arg).expect("Failed to call primop");
    assert_eq!(result.as_int().unwrap(), -7);
  }

  #[test]
  #[serial]
  fn test_primop_into_value_string() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    // A primop that returns a constant string
    let primop = PrimOp::new(&ctx, "hello", 1, None, |_args, ret| {
      ret.set_string("hello from primop")
    })
    .expect("Failed to create primop");

    let func = primop
      .into_value(&state)
      .expect("Failed to embed primop as value");

    let arg = state.make_null().unwrap();
    let result = func.call(&arg).expect("Failed to call primop");
    assert_eq!(result.as_string().unwrap(), "hello from primop");
  }

  #[test]
  #[serial]
  fn test_primop_arg_as_list() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let list_val = state
      .eval_from_string("[10 20 30]", "<eval>")
      .expect("Failed to evaluate list");

    // A primop that reads list args and returns the sum
    let primop = PrimOp::new(&ctx, "list_sum", 1, None, |args, ret| {
      let list = args[0].as_list()?;
      let mut sum = 0i64;
      for i in 0..list.len() {
        sum += list.get(i)?.as_int()?;
      }
      ret.set_int(sum)
    })
    .expect("Failed to create primop");

    let func = primop
      .into_value(&state)
      .expect("Failed to embed primop as value");

    let result = func.call(&list_val).expect("Failed to call primop");
    assert_eq!(result.as_int().unwrap(), 60);
  }

  #[test]
  #[serial]
  fn test_primop_arg_as_attrs() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let attrs_val = state
      .eval_from_string("{ foo = 42; bar = 13; }", "<eval>")
      .expect("Failed to evaluate attrs");

    // A primop that sums two named attributes
    let primop = PrimOp::new(&ctx, "attr_sum", 1, None, |args, ret| {
      let attrs = args[0].as_attrs()?;
      let foo = attrs.get("foo")?.as_int()?;
      let bar = attrs.get("bar")?.as_int()?;
      ret.set_int(foo + bar)
    })
    .expect("Failed to create primop");

    let func = primop
      .into_value(&state)
      .expect("Failed to embed primop as value");

    let result = func.call(&attrs_val).expect("Failed to call primop");
    assert_eq!(result.as_int().unwrap(), 55);
  }

  #[test]
  #[serial]
  fn test_primop_arg_attrs_has_and_keys() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let attrs_val = state
      .eval_from_string("{ a = 1; b = 2; c = 3; }", "<eval>")
      .expect("Failed to evaluate attrs");

    let primop = PrimOp::new(&ctx, "check_attrs", 1, None, |args, ret| {
      let attrs = args[0].as_attrs()?;
      assert_eq!(attrs.len(), 3);
      assert!(!attrs.is_empty());
      assert!(attrs.has("a")?);
      assert!(attrs.has("b")?);
      assert!(attrs.has("c")?);
      assert!(!attrs.has("zzz")?);
      let keys = attrs.keys()?;
      assert_eq!(keys.len(), 3);
      ret.set_null()
    })
    .expect("Failed to create primop");

    let func = primop
      .into_value(&state)
      .expect("Failed to embed primop as value");

    func.call(&attrs_val).expect("Failed to call primop");
  }

  #[test]
  #[serial]
  fn test_primop_empty_attrs_and_list() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let empty_attrs = state
      .eval_from_string("{}", "<eval>")
      .expect("Failed to evaluate empty attrs");

    let primop = PrimOp::new(&ctx, "empty_check", 1, None, |args, ret| {
      match args[0].as_attrs() {
        Ok(a) => {
          assert!(a.is_empty());
          assert_eq!(a.len(), 0);
        },
        Err(_) => {
          let list = args[0].as_list()?;
          assert!(list.is_empty());
          assert_eq!(list.len(), 0);
        },
      }
      ret.set_null()
    })
    .expect("Failed to create primop");

    let func = primop
      .into_value(&state)
      .expect("Failed to embed primop as value");

    func
      .call(&empty_attrs)
      .expect("call with empty attrs failed");

    let empty_list = state
      .eval_from_string("[]", "<eval>")
      .expect("Failed to evaluate empty list");
    func.call(&empty_list).expect("call with empty list failed");
  }

  #[test]
  #[serial]
  fn test_primop_ret_set_attrs() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));

    // Register a nullary primop that constructs and returns an attrset.
    PrimOp::new(&ctx, "mk_attrs_test", 0, None, |_args, ret| {
      let a = ret.make_int(100)?;
      let b = ret.make_string("hi")?;
      ret.set_attrs(&[("x", &a), ("y", &b)])
    })
    .expect("Failed to create primop")
    .register(&ctx)
    .expect("Failed to register primop");

    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let result = state
      .eval_from_string("builtins.mk_attrs_test", "<eval>")
      .expect("Failed to evaluate expression");
    assert_eq!(result.value_type(), ValueType::Attrs);
    assert_eq!(result.attr_keys().unwrap().len(), 2);
    let x = result.get_attr("x").expect("missing x");
    assert_eq!(x.as_int().unwrap(), 100);
    let y = result.get_attr("y").expect("missing y");
    assert_eq!(y.as_string().unwrap(), "hi");
  }

  #[test]
  #[serial]
  fn test_primop_ret_set_list() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));

    // Register a nullary primop that constructs and returns a list.
    PrimOp::new(&ctx, "mk_list_test", 0, None, |_args, ret| {
      let a = ret.make_int(7)?;
      let b = ret.make_string("hi")?;
      let c = ret.make_bool(true)?;
      ret.set_list(&[&a, &b, &c])
    })
    .expect("Failed to create primop")
    .register(&ctx)
    .expect("Failed to register primop");

    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let result = state
      .eval_from_string("builtins.mk_list_test", "<eval>")
      .expect("Failed to evaluate expression");
    assert_eq!(result.value_type(), ValueType::List);
    assert_eq!(result.list_len().unwrap(), 3);

    let first = result.list_get(0).unwrap();
    assert_eq!(first.as_int().unwrap(), 7);
    let second = result.list_get(1).unwrap();
    assert_eq!(second.as_string().unwrap(), "hi");
    let third = result.list_get(2).unwrap();
    assert!(third.as_bool().unwrap());
  }

  #[test]
  #[serial]
  fn test_primop_ret_make_types() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));

    // Test each make_* method returns correct type and value.
    PrimOp::new(&ctx, "mk_types_test", 0, None, |_args, ret| {
      let int_val = ret.make_int(-42)?;
      assert_eq!(int_val.value_type(), ValueType::Int);
      assert_eq!(int_val.as_int()?, -42);

      let float_val = ret.make_float(3.14)?;
      assert_eq!(float_val.value_type(), ValueType::Float);
      assert!((float_val.as_float()? - 3.14).abs() < 1e-9);

      let bool_val = ret.make_bool(true)?;
      assert_eq!(bool_val.value_type(), ValueType::Bool);
      assert!(bool_val.as_bool()?);

      let null_val = ret.make_null()?;
      assert_eq!(null_val.value_type(), ValueType::Null);

      let str_val = ret.make_string("ok")?;
      assert_eq!(str_val.value_type(), ValueType::String);
      assert_eq!(str_val.as_string()?, "ok");

      ret.set_int(0)
    })
    .expect("Failed to create primop")
    .register(&ctx)
    .expect("Failed to register primop");

    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let result = state
      .eval_from_string("builtins.mk_types_test", "<eval>")
      .expect("Failed to evaluate expression");
    assert_eq!(result.as_int().unwrap(), 0);
  }

  #[test]
  #[serial]
  fn test_primop_value_as_attrs_chained() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let nested = state
      .eval_from_string("{ inner = { x = 99; }; }", "<eval>")
      .expect("Failed to evaluate nested attrs");

    let primop = PrimOp::new(&ctx, "nested_get", 1, None, |args, ret| {
      let outer = args[0].as_attrs()?;
      let inner = outer.get("inner")?;
      let inner_attrs = inner.as_attrs()?;
      let x = inner_attrs.get("x")?.as_int()?;
      ret.set_int(x)
    })
    .expect("Failed to create primop");

    let func = primop
      .into_value(&state)
      .expect("Failed to embed primop as value");

    let result = func.call(&nested).expect("Failed to call primop");
    assert_eq!(result.as_int().unwrap(), 99);
  }
}
