//! Nix flake support.
//!
//! Types:
//!
//! - [`FlakeSettings`]: global flake configuration; pass to
//!   [`EvalStateBuilder::with_flake_settings`](crate::EvalStateBuilder::with_flake_settings).
//! - [`FetchersSettings`]: fetcher configuration required by
//!   [`FlakeReference::parse`] and [`LockedFlake::lock`].
//! - [`FlakeReferenceParseFlags`]: optional flags controlling how a flake
//!   reference string is parsed.
//! - [`LockFlags`]: controls locking behaviour (check, virtual,
//!   write-as-needed, input overrides).
//! - [`FlakeReference`]: an unresolved reference to a flake; produced by
//!   [`FlakeReference::parse`].
//! - [`LockedFlake`]: a fully locked flake; produced by [`LockedFlake::lock`].
//!   Call [`LockedFlake::output_attrs`] to obtain the flake's output attribute
//!   set.

use std::{ffi::CString, ptr::NonNull, sync::Arc};

use crate::{Context, Error, EvalState, Result, Value, check_err, sys};

/// Configuration for the Nix flake subsystem.
///
/// This enables flake evaluation features in the Nix evaluator (such as
/// `builtins.getFlake`). Obtain a `FlakeSettings` and pass it to
/// [`EvalStateBuilder::with_flake_settings`](crate::EvalStateBuilder::with_flake_settings)
/// before building the [`EvalState`].
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
///
/// use nix_bindings::{Context, EvalStateBuilder, Store, flake::FlakeSettings};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let ctx = Arc::new(Context::new()?);
///   let store = Arc::new(Store::open(&ctx, None)?);
///   let flake_settings = FlakeSettings::new(&ctx)?;
///   let state = EvalStateBuilder::new(&store)?
///     .with_flake_settings(&flake_settings)?
///     .build()?;
///
///   Ok(())
/// }
/// ```
pub struct FlakeSettings {
  pub(crate) inner: NonNull<sys::nix_flake_settings>,
  _context:         Arc<Context>,
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

/// Fetcher configuration.
///
/// This is required by [`FlakeReference::parse`] and [`LockedFlake::lock`].
/// Create one with [`FetchersSettings::new`] and keep it alive for the
/// duration of any flake operations that need it.
pub struct FetchersSettings {
  inner:    NonNull<sys::nix_fetchers_settings>,
  _context: Arc<Context>,
}

impl FetchersSettings {
  /// Create new fetcher settings with default values.
  ///
  /// # Errors
  ///
  /// Returns an error if the underlying allocation fails.
  pub fn new(context: &Arc<Context>) -> Result<Self> {
    // SAFETY: context is valid
    let ptr = unsafe { sys::nix_fetchers_settings_new(context.as_ptr()) };
    let inner = NonNull::new(ptr).ok_or(Error::NullPointer)?;
    Ok(FetchersSettings {
      inner,
      _context: Arc::clone(context),
    })
  }

  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::nix_fetchers_settings {
    self.inner.as_ptr()
  }
}

impl Drop for FetchersSettings {
  fn drop(&mut self) {
    // SAFETY: We own the settings and they are valid until drop
    unsafe {
      sys::nix_fetchers_settings_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: FetchersSettings can be shared between threads
unsafe impl Send for FetchersSettings {}
unsafe impl Sync for FetchersSettings {}

/// Flags that control how a flake reference string is parsed.
///
/// Create one with [`FlakeReferenceParseFlags::new`] then optionally call
/// [`set_base_directory`](Self::set_base_directory) before passing it to
/// [`FlakeReference::parse`].
pub struct FlakeReferenceParseFlags {
  inner:    NonNull<sys::nix_flake_reference_parse_flags>,
  _context: Arc<Context>,
}

impl FlakeReferenceParseFlags {
  /// Create new parse flags with default values.
  ///
  /// # Errors
  ///
  /// Returns an error if the underlying allocation fails.
  pub fn new(
    context: &Arc<Context>,
    flake_settings: &FlakeSettings,
  ) -> Result<Self> {
    // SAFETY: context and flake_settings are valid
    let ptr = unsafe {
      sys::nix_flake_reference_parse_flags_new(
        context.as_ptr(),
        flake_settings.as_ptr(),
      )
    };
    let inner = NonNull::new(ptr).ok_or(Error::NullPointer)?;
    Ok(FlakeReferenceParseFlags {
      inner,
      _context: Arc::clone(context),
    })
  }

  /// Set the base directory used when resolving relative flake references.
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails.
  pub fn set_base_directory(self, dir: &str) -> Result<Self> {
    let bytes = dir.as_bytes();
    // SAFETY: context, flags, and dir bytes are valid
    unsafe {
      check_err(
        self._context.as_ptr(),
        sys::nix_flake_reference_parse_flags_set_base_directory(
          self._context.as_ptr(),
          self.inner.as_ptr(),
          bytes.as_ptr().cast(),
          bytes.len(),
        ),
      )?;
    }
    Ok(self)
  }

  pub(crate) unsafe fn as_ptr(
    &self,
  ) -> *mut sys::nix_flake_reference_parse_flags {
    self.inner.as_ptr()
  }
}

impl Drop for FlakeReferenceParseFlags {
  fn drop(&mut self) {
    // SAFETY: We own the flags and they are valid until drop
    unsafe {
      sys::nix_flake_reference_parse_flags_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: FlakeReferenceParseFlags can be shared between threads
unsafe impl Send for FlakeReferenceParseFlags {}
unsafe impl Sync for FlakeReferenceParseFlags {}

/// Flags controlling the lock-file update strategy for [`LockedFlake::lock`].
pub struct LockFlags {
  inner:    NonNull<sys::nix_flake_lock_flags>,
  _context: Arc<Context>,
}

impl LockFlags {
  /// Create new lock flags with default values.
  ///
  /// # Errors
  ///
  /// Returns an error if the underlying allocation fails.
  pub fn new(
    context: &Arc<Context>,
    flake_settings: &FlakeSettings,
  ) -> Result<Self> {
    // SAFETY: context and flake_settings are valid
    let ptr = unsafe {
      sys::nix_flake_lock_flags_new(context.as_ptr(), flake_settings.as_ptr())
    };
    let inner = NonNull::new(ptr).ok_or(Error::NullPointer)?;
    Ok(LockFlags {
      inner,
      _context: Arc::clone(context),
    })
  }

  /// Require the lock file to be up-to-date; fail if it needs updating.
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails.
  pub fn set_mode_check(self) -> Result<Self> {
    // SAFETY: context and flags are valid
    unsafe {
      check_err(
        self._context.as_ptr(),
        sys::nix_flake_lock_flags_set_mode_check(
          self._context.as_ptr(),
          self.inner.as_ptr(),
        ),
      )?;
    }
    Ok(self)
  }

  /// Update the lock file in memory only; do not write it to disk.
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails.
  pub fn set_mode_virtual(self) -> Result<Self> {
    // SAFETY: context and flags are valid
    unsafe {
      check_err(
        self._context.as_ptr(),
        sys::nix_flake_lock_flags_set_mode_virtual(
          self._context.as_ptr(),
          self.inner.as_ptr(),
        ),
      )?;
    }
    Ok(self)
  }

  /// Update and write the lock file to disk if it needs updating.
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails.
  pub fn set_mode_write_as_needed(self) -> Result<Self> {
    // SAFETY: context and flags are valid
    unsafe {
      check_err(
        self._context.as_ptr(),
        sys::nix_flake_lock_flags_set_mode_write_as_needed(
          self._context.as_ptr(),
          self.inner.as_ptr(),
        ),
      )?;
    }
    Ok(self)
  }

  /// Override a specific input with an alternative flake reference.
  ///
  /// `input_path` identifies the input (e.g. `"nixpkgs"`).
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails.
  pub fn add_input_override(
    self,
    input_path: &str,
    flake_ref: &FlakeReference,
  ) -> Result<Self> {
    let path_c = CString::new(input_path)?;
    // SAFETY: context, flags, path_c, and flake_ref are valid
    unsafe {
      check_err(
        self._context.as_ptr(),
        sys::nix_flake_lock_flags_add_input_override(
          self._context.as_ptr(),
          self.inner.as_ptr(),
          path_c.as_ptr(),
          flake_ref.inner.as_ptr(),
        ),
      )?;
    }
    Ok(self)
  }

  pub(crate) unsafe fn as_ptr(&self) -> *mut sys::nix_flake_lock_flags {
    self.inner.as_ptr()
  }
}

impl Drop for LockFlags {
  fn drop(&mut self) {
    // SAFETY: We own the flags and they are valid until drop
    unsafe {
      sys::nix_flake_lock_flags_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: LockFlags can be shared between threads
unsafe impl Send for LockFlags {}
unsafe impl Sync for LockFlags {}

/// Callback that collects a string returned from the Nix C API via a pointer
/// and length pair into an `Option<String>` stored in `user_data`.
unsafe extern "C" fn collect_fragment_cb(
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

/// An unresolved flake reference.
///
/// Obtain one via [`FlakeReference::parse`], then pass it to
/// [`LockedFlake::lock`] (or [`LockFlags::add_input_override`]).
pub struct FlakeReference {
  inner:    NonNull<sys::nix_flake_reference>,
  _context: Arc<Context>,
}

impl FlakeReference {
  /// Parse a flake reference string into a [`FlakeReference`].
  ///
  /// Returns both the parsed reference and any fragment that followed a `#`
  /// in the input string. For references without a fragment the second
  /// element is an empty string.
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails or returns a null pointer.
  pub fn parse(
    context: &Arc<Context>,
    fetch_settings: &FetchersSettings,
    flake_settings: &FlakeSettings,
    parse_flags: &FlakeReferenceParseFlags,
    s: &str,
  ) -> Result<(Self, String)> {
    let bytes = s.as_bytes();

    let mut out_ptr: *mut sys::nix_flake_reference = std::ptr::null_mut();
    let mut fragment: Option<String> = None;

    // SAFETY: all arguments are valid; we capture the fragment via callback
    let err = unsafe {
      sys::nix_flake_reference_and_fragment_from_string(
        context.as_ptr(),
        fetch_settings.as_ptr(),
        flake_settings.as_ptr(),
        parse_flags.as_ptr(),
        bytes.as_ptr().cast(),
        bytes.len(),
        &mut out_ptr as *mut *mut sys::nix_flake_reference,
        Some(collect_fragment_cb),
        &mut fragment as *mut Option<String> as *mut std::os::raw::c_void,
      )
    };

    check_err(unsafe { context.as_ptr() }, err)?;

    let inner = NonNull::new(out_ptr).ok_or(Error::NullPointer)?;

    let frag = fragment.unwrap_or_default();

    Ok((
      FlakeReference {
        inner,
        _context: Arc::clone(context),
      },
      frag,
    ))
  }
}

impl Drop for FlakeReference {
  fn drop(&mut self) {
    // SAFETY: We own the reference and it is valid until drop
    unsafe {
      sys::nix_flake_reference_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: FlakeReference can be shared between threads
unsafe impl Send for FlakeReference {}
unsafe impl Sync for FlakeReference {}

/// A fully locked flake.
///
/// Obtain one via [`LockedFlake::lock`], then call
/// [`output_attrs`](LockedFlake::output_attrs) to get the attribute set of
/// flake outputs.
pub struct LockedFlake {
  inner:    NonNull<sys::nix_locked_flake>,
  _context: Arc<Context>,
}

impl LockedFlake {
  /// Lock a flake, resolving and pinning all inputs.
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails or returns a null pointer.
  pub fn lock(
    context: &Arc<Context>,
    fetch_settings: &FetchersSettings,
    flake_settings: &FlakeSettings,
    eval_state: &EvalState,
    lock_flags: &LockFlags,
    flake_ref: &FlakeReference,
  ) -> Result<Self> {
    // SAFETY: all arguments are valid
    let ptr = unsafe {
      sys::nix_flake_lock(
        context.as_ptr(),
        fetch_settings.as_ptr(),
        flake_settings.as_ptr(),
        eval_state.as_ptr(),
        lock_flags.as_ptr(),
        flake_ref.inner.as_ptr(),
      )
    };

    let inner = NonNull::new(ptr).ok_or(Error::NullPointer)?;

    Ok(LockedFlake {
      inner,
      _context: Arc::clone(context),
    })
  }

  /// Get the output attributes of this locked flake as a Nix value.
  ///
  /// The returned [`Value`] is tied to the lifetime of `eval_state`.
  ///
  /// # Errors
  ///
  /// Returns an error if the C API call fails.
  pub fn output_attrs<'s>(
    &self,
    flake_settings: &FlakeSettings,
    eval_state: &'s EvalState,
  ) -> Result<Value<'s>> {
    // SAFETY: all pointers are valid
    let ptr = unsafe {
      sys::nix_locked_flake_get_output_attrs(
        self._context.as_ptr(),
        flake_settings.as_ptr(),
        eval_state.as_ptr(),
        self.inner.as_ptr(),
      )
    };

    let inner = std::ptr::NonNull::new(ptr).ok_or(Error::NullPointer)?;

    Ok(Value {
      inner,
      state: eval_state,
    })
  }
}

impl Drop for LockedFlake {
  fn drop(&mut self) {
    // SAFETY: We own the locked flake and it is valid until drop
    unsafe {
      sys::nix_locked_flake_free(self.inner.as_ptr());
    }
  }
}

// SAFETY: LockedFlake can be shared between threads
unsafe impl Send for LockedFlake {}
unsafe impl Sync for LockedFlake {}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use serial_test::serial;

  use super::*;
  use crate::{Context, EvalStateBuilder, Store};

  fn make_state(ctx: &Arc<Context>) -> (Arc<Store>, EvalState) {
    let store = Arc::new(Store::open(ctx, None).expect("Failed to open store"));
    let flake_settings =
      FlakeSettings::new(ctx).expect("Failed to create flake settings");
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .with_flake_settings(&flake_settings)
      .expect("Failed to apply flake settings")
      .build()
      .expect("Failed to build state");
    (store, state)
  }

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
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    make_state(&ctx);
  }

  #[test]
  #[serial]
  fn test_fetchers_settings_new() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let _s =
      FetchersSettings::new(&ctx).expect("Failed to create fetcher settings");
  }

  #[test]
  #[serial]
  fn test_flake_reference_parse_flags_new() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let settings =
      FlakeSettings::new(&ctx).expect("Failed to create flake settings");
    let _f = FlakeReferenceParseFlags::new(&ctx, &settings)
      .expect("Failed to create parse flags");
  }

  #[test]
  #[serial]
  fn test_flake_reference_parse_flags_set_base_directory() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let settings =
      FlakeSettings::new(&ctx).expect("Failed to create flake settings");
    let _f = FlakeReferenceParseFlags::new(&ctx, &settings)
      .expect("Failed to create parse flags")
      .set_base_directory("/tmp")
      .expect("Failed to set base directory");
  }

  #[test]
  #[serial]
  fn test_lock_flags_new() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let settings =
      FlakeSettings::new(&ctx).expect("Failed to create flake settings");
    let _f =
      LockFlags::new(&ctx, &settings).expect("Failed to create lock flags");
  }

  #[test]
  #[serial]
  fn test_lock_flags_set_modes() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let settings =
      FlakeSettings::new(&ctx).expect("Failed to create flake settings");
    let _check = LockFlags::new(&ctx, &settings)
      .expect("create")
      .set_mode_check()
      .expect("set_mode_check");
    let _virtual = LockFlags::new(&ctx, &settings)
      .expect("create")
      .set_mode_virtual()
      .expect("set_mode_virtual");
    let _write = LockFlags::new(&ctx, &settings)
      .expect("create")
      .set_mode_write_as_needed()
      .expect("set_mode_write_as_needed");
  }
}
