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

use std::{
  any::TypeId,
  collections::HashSet,
  ffi::CString,
  marker::PhantomData,
  ops::Deref,
  os::raw::{c_int, c_void},
  sync::{LazyLock, Mutex},
};

use crate::{
  Context,
  Error,
  EvalState,
  Result,
  Value,
  check_err,
  check_ptr,
  error::catch_callback,
  sys,
};

/// String dependencies borrowed for one external conversion callback.
///
/// This handle cannot outlive the callback or be sent to another thread.
/// Entries are copied into Nix's context, not retained as Rust references.
pub struct StringContext<'a> {
  inner:   *mut sys::nix_string_context,
  context: Option<Context>,
  _borrow: PhantomData<&'a mut sys::nix_string_context>,
}

impl StringContext<'_> {
  /// Add an encoded Nix string-context entry.
  ///
  /// Opaque entries use the store-path basename (`hash-name`), without the
  /// store directory. This records a dependency; it does not copy a path.
  ///
  /// # Errors
  ///
  /// Returns an error for interior NUL bytes, invalid context entries, or
  /// failure to allocate the callback's private error context.
  pub fn add(&mut self, entry: &str) -> Result<()> {
    let entry = CString::new(entry)?;
    let context = match &mut self.context {
      Some(context) => context,
      slot @ None => slot.insert(Context::new()?),
    };
    // SAFETY: inner is borrowed from this active callback. The private
    // context is live, and Nix copies the entry before returning.
    unsafe {
      check_err(
        context.as_ptr(),
        sys::nix_external_add_string_context(
          context.as_ptr(),
          self.inner,
          entry.as_ptr(),
        ),
      )
    }
  }
}

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

  /// Coerce to a string while recording dependencies and observing Nix's flags.
  ///
  /// The default delegates to [`Self::coerce_to_string`]. `copy_to_store`
  /// requests copying referenced paths to the store, not merely adding them
  /// to `context`. Returning `None`, an empty string, or an error makes Nix
  /// reject the conversion. The C API cannot preserve a custom error message.
  ///
  /// # Errors
  ///
  /// Implementations may return errors from dependency insertion or conversion.
  fn coerce_to_string_with_context(
    &self,
    _context: &mut StringContext<'_>,
    _coerce_more: bool,
    _copy_to_store: bool,
  ) -> Result<Option<String>> {
    Ok(self.coerce_to_string())
  }

  /// Return JSON text, optionally recording dependencies in `context`.
  ///
  /// Nix parses the returned text as JSON. Invalid JSON, interior NUL bytes,
  /// an empty string, `None` (the default), or an error rejects conversion.
  /// No invalid result is replaced with JSON `null`. `strict` and
  /// `copy_to_store` are Nix's conversion flags. Nix copies the returned text.
  ///
  /// # Errors
  ///
  /// Implementations may return errors from dependency insertion or conversion.
  /// The C API reports these as a conversion failure without the Rust message.
  fn to_json(
    &self,
    _context: &mut StringContext<'_>,
    _strict: bool,
    _copy_to_store: bool,
  ) -> Result<Option<String>> {
    Ok(None)
  }

  /// Test equality with another external value of the same Rust type.
  ///
  /// The default implementation returns `false` (values are never equal).
  fn equal(&self, _other: &Self) -> bool {
    false
  }
}

// Equality may receive arbitrary foreign C API payloads. Check their address
// without reading memory until we know they belong to this crate.
static PAYLOADS: LazyLock<Mutex<HashSet<usize>>> =
  LazyLock::new(|| Mutex::new(HashSet::new()));

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

  coerce_fn: unsafe fn(
    *const c_void,
    &mut StringContext<'_>,
    bool,
    bool,
  ) -> Result<Option<String>>,
  json_fn: unsafe fn(
    *const c_void,
    &mut StringContext<'_>,
    bool,
    bool,
  ) -> Result<Option<String>>,

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
  ptr: *const c_void,
  context: &mut StringContext<'_>,
  coerce_more: bool,
  copy_to_store: bool,
) -> Result<Option<String>> {
  // SAFETY: the dispatch pointer is paired with the boxed T in new.
  let value = unsafe { &*ptr.cast::<T>() };
  value.coerce_to_string_with_context(context, coerce_more, copy_to_store)
}

unsafe fn json_erased<T: NixExternal>(
  ptr: *const c_void,
  context: &mut StringContext<'_>,
  strict: bool,
  copy_to_store: bool,
) -> Result<Option<String>> {
  // SAFETY: the dispatch pointer is paired with the boxed T in new.
  let value = unsafe { &*ptr.cast::<T>() };
  value.to_json(context, strict, copy_to_store)
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
  fn new<T: NixExternal>(value: T) -> Box<Self> {
    let payload = Box::new(Self {
      type_id:      TypeId::of::<T>(),
      data:         Box::into_raw(Box::new(value)).cast::<c_void>(),
      drop_fn:      drop_erased::<T>,
      display_fn:   display_erased::<T>,
      type_name_fn: type_name_erased::<T>,
      coerce_fn:    coerce_erased::<T>,
      json_fn:      json_erased::<T>,
      equal_fn:     equal_erased::<T>,
    });
    PAYLOADS
      .lock()
      .unwrap_or_else(|error| error.into_inner())
      .insert((&*payload as *const Self) as usize);
    payload
  }

  /// The caller must keep the external value owning this payload alive.
  unsafe fn try_from_void<'a>(ptr: *mut c_void) -> Option<&'a Self> {
    let guard = PAYLOADS.lock().unwrap_or_else(|error| error.into_inner());
    if !guard.contains(&(ptr as usize)) {
      return None;
    }
    // SAFETY: membership establishes the layout; the caller keeps it live.
    Some(unsafe { Self::from_void(ptr) })
  }

  /// Only accepts this crate's live external payload pointers.
  unsafe fn from_void<'a>(ptr: *mut c_void) -> &'a Self {
    // SAFETY: the caller guarantees ownership and lifetime of this payload.
    unsafe { &*ptr.cast::<Self>() }
  }
}

impl Drop for ErasedPayload {
  fn drop(&mut self) {
    PAYLOADS
      .lock()
      .unwrap_or_else(|error| error.into_inner())
      .remove(&((self as *const Self) as usize));
    // SAFETY: self.data was created by Box::into_raw::<T> and drop_fn
    // knows its original concrete type T.
    unsafe { (self.drop_fn)(self.data) };
  }
}

unsafe extern "C" fn drop_payload_finalizer(
  _obj: *mut c_void,
  data: *mut c_void,
) {
  let _ = catch_callback(|| {
    // SAFETY: the finalizer exclusively owns this Box after make_external.
    drop(unsafe { Box::from_raw(data.cast::<ErasedPayload>()) });
  });
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
    let _ = catch_callback(|| {
      let payload = unsafe { ErasedPayload::from_void(self_) };
      let s = unsafe { (payload.display_fn)(payload.data) };
      if let Ok(cs) = CString::new(s)
        && let Ok(context) = Context::new()
      {
        // SAFETY: printer is borrowed for this call; a private error context
        // prevents C++ exceptions from crossing the Rust callback on failure.
        unsafe {
          sys::nix_external_print(context.as_ptr(), printer, cs.as_ptr());
        }
      }
    });
  }

  unsafe extern "C" fn show_type(
    self_: *mut std::os::raw::c_void,
    res: *mut sys::nix_string_return,
  ) {
    let _ = catch_callback(|| {
      let payload = unsafe { ErasedPayload::from_void(self_) };
      let name = unsafe { (payload.type_name_fn)(payload.data) };
      if let Ok(cs) = CString::new(name) {
        unsafe { sys::nix_set_string_return(res, cs.as_ptr()) };
      }
    });
  }

  unsafe extern "C" fn type_of(
    _self: *mut std::os::raw::c_void,
    res: *mut sys::nix_string_return,
  ) {
    // SAFETY: Nix supplies the return slot and copies this static C string.
    unsafe { sys::nix_set_string_return(res, c"nix-external".as_ptr()) };
  }

  unsafe extern "C" fn coerce_to_string(
    self_: *mut c_void,
    context: *mut sys::nix_string_context,
    coerce_more: c_int,
    copy_to_store: c_int,
    res: *mut sys::nix_string_return,
  ) {
    let _ = catch_callback(|| {
      // SAFETY: Nix supplies this crate's live payload and a context borrowed
      // for the callback. No wrapper or raw context pointer escapes the call.
      let payload = unsafe { ErasedPayload::from_void(self_) };
      let mut context = StringContext {
        inner:   context,
        context: None,
        _borrow: PhantomData,
      };
      if let Ok(Some(text)) = unsafe {
        (payload.coerce_fn)(
          payload.data,
          &mut context,
          coerce_more != 0,
          copy_to_store != 0,
        )
      } && let Ok(text) = CString::new(text)
      {
        // SAFETY: the C API copies text before this CString is dropped.
        unsafe { sys::nix_set_string_return(res, text.as_ptr()) };
      }
    });
  }

  unsafe extern "C" fn print_json(
    self_: *mut c_void,
    _state: *mut sys::EvalState,
    strict: bool,
    context: *mut sys::nix_string_context,
    copy_to_store: bool,
    res: *mut sys::nix_string_return,
  ) {
    let _ = catch_callback(|| {
      // SAFETY: payload and context remain live for the duration of this call.
      let payload = unsafe { ErasedPayload::from_void(self_) };
      let mut context = StringContext {
        inner:   context,
        context: None,
        _borrow: PhantomData,
      };
      if let Ok(Some(text)) = unsafe {
        (payload.json_fn)(payload.data, &mut context, strict, copy_to_store)
      } && let Ok(text) = CString::new(text)
      {
        // SAFETY: the API copies and then parses the text as JSON. Leaving
        // res unset on failure asks Nix to throw, never to return JSON null.
        unsafe { sys::nix_set_string_return(res, text.as_ptr()) };
      }
    });
  }

  unsafe extern "C" fn equal(
    self_: *mut std::os::raw::c_void,
    other: *mut std::os::raw::c_void,
  ) -> std::os::raw::c_int {
    catch_callback(|| {
      // SAFETY: Nix keeps both external objects live through the comparison.
      let a = unsafe { ErasedPayload::from_void(self_) };
      let Some(b) = (unsafe { ErasedPayload::try_from_void(other) }) else {
        return 0;
      };
      if a.type_id != b.type_id {
        return 0;
      }
      // SAFETY: type equality establishes the concrete type of both boxes.
      c_int::from(unsafe { (a.equal_fn)(a.data, b.data) })
    })
    .unwrap_or(0)
  }

  // XML is unsupported: the C API provides no XML document-writing bindings.
  sys::NixCExternalValueDesc {
    print:            Some(print),
    showType:         Some(show_type),
    typeOf:           Some(type_of),
    coerceToString:   Some(coerce_to_string),
    equal:            Some(equal),
    printValueAsJSON: Some(print_json),
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
    let v = self.alloc_value()?;
    let mut payload = ErasedPayload::new(data);
    // Cast away the const on the vtable: the C API takes *mut but never
    // mutates the descriptor itself.
    let vtable_ptr = &VTABLE as *const sys::NixCExternalValueDesc
      as *mut sys::NixCExternalValueDesc;

    // SAFETY: the static descriptor and boxed payload stay live. Ownership
    // transfers to the finalizer only after external allocation succeeds.
    let ext_ptr = unsafe {
      sys::nix_create_external_value(
        self.context.as_ptr(),
        vtable_ptr,
        (&mut *payload as *mut ErasedPayload).cast::<c_void>(),
      )
    };

    let ext_ptr =
      check_ptr(unsafe { self.context.as_ptr() }, ext_ptr)?.as_ptr();
    // SAFETY: the external GC object is rooted until the decref below. Its
    // finalizer now owns the payload even if nix_init_external fails.
    unsafe {
      sys::nix_gc_register_finalizer(
        ext_ptr.cast::<c_void>(),
        Box::into_raw(payload).cast::<c_void>(),
        Some(drop_payload_finalizer),
      );
      let initialized = check_err(
        self.context.as_ptr(),
        sys::nix_init_external(
          self.context.as_ptr(),
          v.inner.as_ptr(),
          ext_ptr,
        ),
      );
      let released = check_err(
        self.context.as_ptr(),
        sys::nix_gc_decref(self.context.as_ptr(), ext_ptr.cast::<c_void>()),
      );
      initialized?;
      released?;
    }

    Ok(ExternalValueHandle { value: v, ext_ptr })
  }
}

/// A handle to a Nix external value that retains the `ExternalValue*`.
///
/// This type stores the `ExternalValue*` from `nix_create_external_value`
/// directly and uses `nix_get_external_value_content` for downcasting.
///
/// Derefs to [`Value`].
pub struct ExternalValueHandle<'s> {
  value:   Value<'s>,
  ext_ptr: *mut sys::ExternalValue,
}

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
