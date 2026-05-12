//! External Nix values backed by Rust data.
//!
//! - [`NixExternal`]: trait your data type must implement.
//! - [`EvalState::make_external`](crate::EvalState::make_external): wraps a
//!   value and returns an [`ExternalValueHandle`].
//! - [`ExternalValueHandle::as_external`]: downcasts back to a concrete Rust
//!   type.
//!
//! A [`TypeId`] is stored alongside each data pointer.
//! [`ExternalValueHandle::as_external`] checks it before returning a reference,
//! so a wrong-type downcast returns [`Error::InvalidType`] rather than UB.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::sync::Arc;
//!
//! use nix_bindings::{Context, EvalStateBuilder, Store, external::NixExternal};
//!
//! struct MyData(i64);
//!
//! impl NixExternal for MyData {
//!   fn display(&self) -> String {
//!     format!("MyData({})", self.0)
//!   }
//!   fn type_name(&self) -> &'static str {
//!     "MyData"
//!   }
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!   let ctx = Arc::new(Context::new()?);
//!   let store = Arc::new(Store::open(&ctx, None)?);
//!   let state = EvalStateBuilder::new(&store)?.build()?;
//!
//!   let handle = state.make_external(MyData(42))?;
//!   let back = handle.as_external::<MyData>()?;
//!   assert_eq!(back.0, 42);
//!
//!   Ok(())
//! }
//! ```

use std::{any::TypeId, ffi::CString, ops::Deref};

use crate::{Error, EvalState, Result, Value, sys};

/// A Rust type that can be embedded as an external value in the Nix evaluator.
///
/// Implementing this trait is the only requirement for storing your type inside
/// Nix values via [`EvalState::make_external`]. All methods except
/// [`display`](NixExternal::display) and [`type_name`](NixExternal::type_name)
/// have default no-op implementations.
pub trait NixExternal: Send + Sync + 'static {
  /// Return a human-readable representation of the value.
  ///
  /// This is called when Nix prints the value (e.g. in the REPL).
  fn display(&self) -> String;

  /// Return the type name shown by `:t` in the Nix REPL.
  fn type_name(&self) -> &'static str;

  /// Try to coerce the value to a string.
  ///
  /// Return `Some(s)` to allow coercion; return `None` (the default) to have
  /// Nix throw an error when coercion is attempted.
  fn coerce_to_string(&self) -> Option<String> {
    None
  }

  /// Test equality with another external value of the same Rust type.
  ///
  /// The default implementation returns `false` (values are never equal).
  fn equal(&self, _other: &Self) -> bool {
    false
  }
}

/// Heap-allocated wrapper that combines a boxed `T` with its [`TypeId`].
///
/// This is the allocation stored behind the `void*` data pointer that is
/// passed to [`nix_create_external_value`](sys::nix_create_external_value).
struct ErasedPayload {
  type_id: TypeId,

  // The concrete data follows; we keep only a raw pointer into the T.
  data: *mut std::os::raw::c_void,

  // Destructor: how to drop the original Box<T>.
  drop_fn: unsafe fn(*mut std::os::raw::c_void),

  // display(): returns an owned String.
  display_fn: unsafe fn(*const std::os::raw::c_void) -> String,

  // type_name(): returns a &'static str.
  type_name_fn: unsafe fn(*const std::os::raw::c_void) -> &'static str,

  // coerce_to_string(): returns Option<String>.
  coerce_fn: unsafe fn(*const std::os::raw::c_void) -> Option<String>,

  // equal(other_data): compare two ErasedPayload.data pointers of the same T.
  equal_fn:
    unsafe fn(*const std::os::raw::c_void, *const std::os::raw::c_void) -> bool,
}

unsafe fn drop_erased<T>(ptr: *mut std::os::raw::c_void) {
  drop(unsafe { Box::from_raw(ptr.cast::<T>()) });
}

unsafe fn display_erased<T: NixExternal>(
  ptr: *const std::os::raw::c_void,
) -> String {
  let t = unsafe { &*(ptr as *const T) };
  t.display()
}

unsafe fn type_name_erased<T: NixExternal>(
  ptr: *const std::os::raw::c_void,
) -> &'static str {
  let t = unsafe { &*(ptr as *const T) };
  t.type_name()
}

unsafe fn coerce_erased<T: NixExternal>(
  ptr: *const std::os::raw::c_void,
) -> Option<String> {
  let t = unsafe { &*(ptr as *const T) };
  t.coerce_to_string()
}

unsafe fn equal_erased<T: NixExternal>(
  ptr: *const std::os::raw::c_void,
  other: *const std::os::raw::c_void,
) -> bool {
  let a = unsafe { &*(ptr as *const T) };
  let b = unsafe { &*(other as *const T) };
  a.equal(b)
}

impl ErasedPayload {
  fn new<T: NixExternal>(value: T) -> *mut Self {
    let data_box = Box::new(value);
    let data_ptr = Box::into_raw(data_box) as *mut std::os::raw::c_void;

    Box::into_raw(Box::new(ErasedPayload {
      type_id:      TypeId::of::<T>(),
      data:         data_ptr,
      drop_fn:      drop_erased::<T>,
      display_fn:   display_erased::<T>,
      type_name_fn: type_name_erased::<T>,
      coerce_fn:    coerce_erased::<T>,
      equal_fn:     equal_erased::<T>,
    }))
  }

  unsafe fn from_void<'a>(ptr: *mut std::os::raw::c_void) -> &'a Self {
    unsafe { &*(ptr as *const ErasedPayload) }
  }
}

impl Drop for ErasedPayload {
  fn drop(&mut self) {
    // SAFETY: self.data was created by Box::into_raw::<T> and drop_fn
    // knows its original concrete type T.
    unsafe { (self.drop_fn)(self.data) };
  }
}

/// The static vtable passed to `nix_create_external_value`.
///
/// We store only one global vtable because all per-type dispatch happens
/// through the function pointers embedded in [`ErasedPayload`].
static VTABLE: sys::NixCExternalValueDesc = {
  unsafe extern "C" fn print(
    self_: *mut std::os::raw::c_void,
    printer: *mut sys::nix_printer,
  ) {
    let payload = unsafe { ErasedPayload::from_void(self_) };
    let s = unsafe { (payload.display_fn)(payload.data) };
    if let Ok(cs) = CString::new(s) {
      unsafe {
        sys::nix_external_print(std::ptr::null_mut(), printer, cs.as_ptr());
      }
    }
  }

  unsafe extern "C" fn show_type(
    self_: *mut std::os::raw::c_void,
    res: *mut sys::nix_string_return,
  ) {
    let payload = unsafe { ErasedPayload::from_void(self_) };
    let name = unsafe { (payload.type_name_fn)(payload.data) };
    if let Ok(cs) = CString::new(name) {
      unsafe { sys::nix_set_string_return(res, cs.as_ptr()) };
    }
  }

  unsafe extern "C" fn type_of(
    _self: *mut std::os::raw::c_void,
    res: *mut sys::nix_string_return,
  ) {
    // builtins.typeOf for all external values returns "nix-external".
    if let Ok(cs) = CString::new("nix-external") {
      unsafe { sys::nix_set_string_return(res, cs.as_ptr()) };
    }
  }

  unsafe extern "C" fn coerce_to_string(
    self_: *mut std::os::raw::c_void,
    _c: *mut sys::nix_string_context,
    _coerce_more: std::os::raw::c_int,
    _copy_to_store: std::os::raw::c_int,
    res: *mut sys::nix_string_return,
  ) {
    let payload = unsafe { ErasedPayload::from_void(self_) };
    if let Some(s) = unsafe { (payload.coerce_fn)(payload.data) }
      && let Ok(cs) = CString::new(s)
    {
      unsafe { sys::nix_set_string_return(res, cs.as_ptr()) };
    }
  }

  unsafe extern "C" fn equal(
    self_: *mut std::os::raw::c_void,
    other: *mut std::os::raw::c_void,
  ) -> std::os::raw::c_int {
    let a = unsafe { ErasedPayload::from_void(self_) };
    let b = unsafe { ErasedPayload::from_void(other) };
    // Only compare if they share the same concrete type.
    if a.type_id != b.type_id {
      return 0;
    }
    if unsafe { (a.equal_fn)(a.data, b.data) } {
      1
    } else {
      0
    }
  }

  // JSON and XML printing default to not-implemented (None).
  sys::NixCExternalValueDesc {
    print:            Some(print),
    showType:         Some(show_type),
    typeOf:           Some(type_of),
    coerceToString:   Some(coerce_to_string),
    equal:            Some(equal),
    printValueAsJSON: None,
    printValueAsXML:  None,
  }
};

impl EvalState {
  /// Wrap a [`NixExternal`] value and return an [`ExternalValueHandle`].
  ///
  /// The handle carries the Nix [`Value`] and the raw `ExternalValue*` needed
  /// for downcasting via [`ExternalValueHandle::as_external`]. The value's
  /// lifetime is managed by the Nix GC.
  ///
  /// # Errors
  ///
  /// Returns an error if value allocation or external value creation fails.
  pub fn make_external<T: NixExternal>(
    &self,
    data: T,
  ) -> Result<ExternalValueHandle<'_>> {
    let payload_ptr = ErasedPayload::new(data);
    // Cast away the const on the vtable: the C API takes *mut but never
    // mutates the descriptor itself.
    let vtable_ptr = &VTABLE as *const sys::NixCExternalValueDesc
      as *mut sys::NixCExternalValueDesc;

    // Allocate the nix_value before touching the GC with the external pointer.
    let v = self.alloc_value()?;

    // SAFETY: vtable_ptr points to the static vtable; payload_ptr is a
    // freshly allocated ErasedPayload.
    let ext_ptr = unsafe {
      sys::nix_create_external_value(
        self.context.as_ptr(),
        vtable_ptr,
        payload_ptr.cast::<std::os::raw::c_void>(),
      )
    };

    if ext_ptr.is_null() {
      drop(unsafe { Box::from_raw(payload_ptr) });
      return Err(Error::NullPointer);
    }

    // SAFETY: context, value, and external value pointer are valid.
    unsafe {
      crate::check_err(
        self.context.as_ptr(),
        sys::nix_init_external(
          self.context.as_ptr(),
          v.inner.as_ptr(),
          ext_ptr,
        ),
      )?;
    }

    Ok(ExternalValueHandle { value: v, ext_ptr })
  }
}

/// A handle to a Nix external value that retains the `ExternalValue*`.
///
/// `nix_get_external` is broken in Nix 2.32.7; see the module-level note.
/// This type stores the `ExternalValue*` from `nix_create_external_value`
/// directly and uses `nix_get_external_value_content` for downcasting.
///
/// Derefs to [`Value`].
pub struct ExternalValueHandle<'s> {
  value:   Value<'s>,
  ext_ptr: *mut sys::ExternalValue,
}

// SAFETY: ExternalValue* is GC-managed. T: Send + Sync is enforced by
// NixExternal.
unsafe impl Send for ExternalValueHandle<'_> {}
unsafe impl Sync for ExternalValueHandle<'_> {}

impl<'s> Deref for ExternalValueHandle<'s> {
  type Target = Value<'s>;

  fn deref(&self) -> &Self::Target {
    &self.value
  }
}

impl ExternalValueHandle<'_> {
  /// Downcast to a concrete Rust type `T`.
  ///
  /// # Errors
  ///
  /// Returns [`Error::InvalidType`] if the stored
  /// [`TypeId`] does not match `T`.
  pub fn as_external<T: NixExternal>(&self) -> Result<&T> {
    // SAFETY: ext_ptr was returned by nix_create_external_value and is kept
    // alive by the nix_value that this handle owns.
    let void_ptr = unsafe {
      sys::nix_get_external_value_content(
        self.value.state.context.as_ptr(),
        self.ext_ptr,
      )
    };

    if void_ptr.is_null() {
      return Err(Error::NullPointer);
    }

    // SAFETY: void_ptr points to the ErasedPayload allocated in
    // ErasedPayload::new.
    let payload = unsafe { ErasedPayload::from_void(void_ptr) };

    if payload.type_id != TypeId::of::<T>() {
      return Err(Error::InvalidType {
        expected: std::any::type_name::<T>(),
        actual:   "external value of different type".to_string(),
      });
    }

    // SAFETY: type_id matches, so data is a valid *mut T.
    let t_ref = unsafe { &*(payload.data as *const T) };
    Ok(t_ref)
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use serial_test::serial;

  use super::*;
  use crate::{Context, EvalStateBuilder, Store, ValueType};

  struct MyData(i64);

  impl NixExternal for MyData {
    fn display(&self) -> String {
      format!("MyData({})", self.0)
    }

    fn type_name(&self) -> &'static str {
      "MyData"
    }
  }

  fn make_eval_state() -> (Arc<Context>, Arc<Store>, crate::EvalState) {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");
    (ctx, store, state)
  }

  #[test]
  #[serial]
  fn test_make_and_recover_external() {
    let (_ctx, _store, state) = make_eval_state();

    let handle = state
      .make_external(MyData(42))
      .expect("make_external failed");
    assert_eq!(handle.value_type(), ValueType::External);

    let back = handle.as_external::<MyData>().expect("as_external failed");
    assert_eq!(back.0, 42);
  }

  #[test]
  #[serial]
  fn test_wrong_type_returns_error() {
    let (_ctx, _store, state) = make_eval_state();

    struct OtherData;
    impl NixExternal for OtherData {
      fn display(&self) -> String {
        "OtherData".to_string()
      }

      fn type_name(&self) -> &'static str {
        "OtherData"
      }
    }

    let handle = state
      .make_external(MyData(1))
      .expect("make_external failed");
    let result = handle.as_external::<OtherData>();
    assert!(
      result.is_err(),
      "Downcasting to wrong type should return Err"
    );
  }

  #[test]
  #[serial]
  fn test_as_external_on_non_external_value() {
    let (_ctx, _store, state) = make_eval_state();

    let int_val = state.make_int(5).expect("make_int failed");
    // int_val is a Value, not an ExternalValueHandle; just confirm it's not
    // External type
    assert_ne!(int_val.value_type(), ValueType::External);
  }
}
