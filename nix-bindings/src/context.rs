//! [`Context`] (Nix C-API context handle), [`Verbosity`], and free
//! helpers that read process-global Nix state.

#![cfg(feature = "store")]

use std::{
  ffi::{CStr, CString},
  ptr::NonNull,
};

use crate::{
  Error, Result,
  error::{check_err, string_from_callback},
  sys,
};

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

/// Returns `true` when Nix is running in pure evaluation mode (`--pure-eval`).
///
/// Reads the global `pure-eval` Nix setting. The result reflects the setting
/// at call time and is stable for the lifetime of a single Nix evaluation.
///
/// # Use in primops
///
/// [`primop::PrimOpRet::set_path`](crate::primop::PrimOpRet::set_path) and
/// [`primop::PrimOpRet::make_path`](crate::primop::PrimOpRet::make_path)
/// call `nix_init_path_string` internally, which the Nix evaluator rejects
/// for absolute paths in pure mode. Check this function first so your
/// primop can make the right call: return an explicit error, use a string,
/// or refuse to run at all.
pub fn is_pure_eval() -> bool {
  // SAFETY: nix_setting_get is safe with a null context; errors are silently
  // ignored, and a missing key means the default (false).
  unsafe {
    let val = string_from_callback(|cb, ud| {
      sys::nix_setting_get(std::ptr::null_mut(), c"pure-eval".as_ptr(), cb, ud);
    });
    val.as_deref() == Some("true")
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
    Self::new_inner(false)
  }

  /// Create a new Nix context without loading user/environment
  /// configuration (`nix.conf`, `NIX_*` env vars).
  ///
  /// Equivalent to [`new`](Self::new) but uses
  /// `nix_libstore_init_no_load_config` so the store is initialized in a
  /// deterministic state. Useful in tests and sandboxed callers.
  ///
  /// Note: this affects only the *first* context created in the process;
  /// subsequent calls (including [`new`](Self::new)) hit the one-shot
  /// init gate and reuse whatever mode was selected first.
  ///
  /// # Errors
  ///
  /// Returns an error if context creation or library initialization fails.
  pub fn new_no_load_config() -> Result<Self> {
    Self::new_inner(true)
  }

  fn new_inner(no_load_config: bool) -> Result<Self> {
    // SAFETY: nix_c_context_create is safe to call
    let ctx_ptr = unsafe { sys::nix_c_context_create() };
    let inner = NonNull::new(ctx_ptr).ok_or(Error::NullPointer)?;

    let ctx = Context { inner };

    // The nix_lib*_init functions are documented as one-shot. Reinvoking
    // them across Contexts is a waste at best and a race at worst; the
    // Once gate ensures exactly one initialization per process.
    static INIT: std::sync::Once = std::sync::Once::new();
    let mut init_err: Result<()> = Ok(());
    INIT.call_once(|| {
      // SAFETY: nix_lib*_init are safe with a fresh context.
      let r = (|| unsafe {
        check_err(
          ctx.inner.as_ptr(),
          sys::nix_libutil_init(ctx.inner.as_ptr()),
        )?;
        let store_err = if no_load_config {
          sys::nix_libstore_init_no_load_config(ctx.inner.as_ptr())
        } else {
          sys::nix_libstore_init(ctx.inner.as_ptr())
        };
        check_err(ctx.inner.as_ptr(), store_err)?;
        check_err(
          ctx.inner.as_ptr(),
          sys::nix_libexpr_init(ctx.inner.as_ptr()),
        )
      })();
      init_err = r;
    });
    init_err?;

    Ok(ctx)
  }

  /// Set a global Nix configuration setting.
  ///
  /// Settings take effect for new [`EvalState`](crate::EvalState) instances.
  /// Use `"extra-<name>"` to append to an existing setting's value.
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
  /// Note: Nix's verbosity is **process-global**; the `&self` receiver is
  /// for ergonomic API consistency. Calling on any [`Context`] affects
  /// every other context in the same process.
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

  /// Clear any error state currently stored on the context.
  ///
  /// Nix's C API parks the last error message and code on the context.
  /// Code paths that signal failure by returning a null pointer (e.g.
  /// [`Value::get_attr`](crate::Value::get_attr)) inspect that buffer to
  /// decide whether the null was a genuine failure or just an absent
  /// value, so a stale message from an earlier unrelated call can confuse
  /// them. Call this between recoverable operations to reset the buffer.
  pub fn clear_error(&self) {
    // SAFETY: context is valid for the lifetime of self.
    unsafe { sys::nix_clear_err(self.inner.as_ptr()) };
  }

  /// Load Nix's configured plugins.
  ///
  /// Reads the `plugin-files` setting and loads each entry. Must be called
  /// before any [`EvalState`](crate::EvalState) is built for plugins to be
  /// visible.
  ///
  /// # Errors
  ///
  /// Returns an error if plugin loading fails.
  #[cfg(feature = "main")]
  pub fn init_plugins(&self) -> Result<()> {
    // SAFETY: context is valid
    unsafe {
      check_err(
        self.inner.as_ptr(),
        sys::nix_init_plugins(self.inner.as_ptr()),
      )
    }
  }

  /// Set the log format Nix uses when writing log messages.
  ///
  /// Format strings recognised by Nix include `"raw"`, `"internal-json"`,
  /// `"bar"`, and `"bar-with-logs"`. Setting is process-global, see
  /// [`set_verbosity`](Self::set_verbosity).
  ///
  /// # Errors
  ///
  /// Returns an error if the format is unrecognised or the underlying
  /// call fails.
  #[cfg(feature = "main")]
  pub fn set_log_format(&self, format: &str) -> Result<()> {
    let format_c = CString::new(format)?;
    // SAFETY: context and format are valid
    unsafe {
      check_err(
        self.inner.as_ptr(),
        sys::nix_set_log_format(self.inner.as_ptr(), format_c.as_ptr()),
      )
    }
  }

  /// Run a Nix GC cycle now.
  ///
  /// Useful in tests and long-running workers that hold onto large value
  /// graphs. Cheap to call repeatedly; Nix's GC is incremental.
  #[cfg(feature = "expr")]
  pub fn gc_now() {
    // SAFETY: nix_gc_now takes no arguments.
    unsafe { sys::nix_gc_now() };
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

// SAFETY: `nix_c_context` is exclusively-owned mutable state. Moving the
// pointer to another thread is sound provided the source thread releases
// ownership; the Box-like value semantics of `Context` already enforce
// that at the type level. `Sync` is NOT implemented: every C entry point
// rewrites the per-context error buffer, so two threads holding
// `&Context` would race on that buffer with no synchronization.
unsafe impl Send for Context {}
