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

use std::{ffi::CString, marker::PhantomData, os::raw::c_void};

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
  // The pointers are borrowed from Nix and must NOT be
  // decreffed by our code.
  let arg_slice = unsafe { std::slice::from_raw_parts(args, data.arity) };
  let arg_wrappers: Vec<PrimOpArg<'_>> = arg_slice
    .iter()
    .map(|&p| {
      PrimOpArg {
        inner: p,
        ctx: context,
        state,
        _phantom: PhantomData,
      }
    })
    .collect();

  let mut ret_wrapper = PrimOpRet {
    inner:    ret,
    ctx:      context,
    _phantom: PhantomData,
  };

  if let Err(e) = (data.f)(&arg_wrappers, &mut ret_wrapper) {
    // Propagate the error to the Nix context so it surfaces as a Nix
    // evaluation error.  Fall back to a static message if the error string
    // itself contains an interior NUL byte (which would make CString::new
    // fail).
    let msg = format!("primop error: {e}");
    let msg_c = CString::new(msg)
      .unwrap_or_else(|_| CString::new("primop error").unwrap());
    unsafe {
      sys::nix_set_err_msg(
        context,
        sys::nix_err_NIX_ERR_NIX_ERROR,
        msg_c.as_ptr(),
      );
    }
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
  pub fn force(&mut self) -> Result<()> {
    unsafe {
      check_err(
        self.ctx,
        sys::nix_value_force(self.ctx, self.state, self.inner),
      )
    }
  }

  /// Extract this argument as an integer.
  ///
  /// # Errors
  ///
  /// Returns [`Error::InvalidType`] if the value is not an integer.
  pub fn as_int(&self) -> Result<i64> {
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
  /// # Errors
  ///
  /// Returns [`Error::InvalidType`] if the value is not a float.
  pub fn as_float(&self) -> Result<f64> {
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
  /// # Errors
  ///
  /// Returns [`Error::InvalidType`] if the value is not a boolean.
  pub fn as_bool(&self) -> Result<bool> {
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
  /// # Errors
  ///
  /// Returns [`Error::InvalidType`] if the value is not a string, or an
  /// error if the string contains invalid UTF-8.
  pub fn as_string(&self) -> Result<String> {
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
}

/// The writable return-value slot provided to a primop closure.
///
/// Exactly one `set_*` method must be called before returning `Ok(())`.
pub struct PrimOpRet<'a> {
  inner:    *mut sys::nix_value,
  ctx:      *mut sys::nix_c_context,
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
  /// Context needed to call `nix_gc_decref` in Drop.
  ctx:        *mut sys::nix_c_context,
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
    context: &Context,
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
    let doc_ptr = doc_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());

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
      // Allocation failed; free the closure ourselves.
      let _ = unsafe { Box::from_raw(data_raw as *mut ClosureData) };
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
      ctx:        unsafe { context.as_ptr() },
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
    check_err(self.ctx, err)?;
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
        let _ = sys::nix_gc_decref(self.ctx, self.inner as *const c_void);
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
}
