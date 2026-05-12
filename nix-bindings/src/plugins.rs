//! Plugin loading and log-format configuration.
//!
//! This module exposes the two functions from `nix_api_main.h` as methods on
//! [`Context`](crate::Context). It is enabled by the `main` feature flag.
//!
//! # Usage
//!
//! ```rust,no_run
//! use std::sync::Arc;
//!
//! use nix_bindings::{Context, EvalStateBuilder, Store};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!   // Use new_no_config() so that settings we set below aren't overridden.
//!   let ctx = Arc::new(Context::new_no_config()?);
//!
//!   // Optionally configure plugin paths before loading.
//!   ctx.set_setting("plugin-files", "/path/to/myplugin.so")?;
//!
//!   // Load plugins after all settings have been configured and before
//!   // creating an EvalState.
//!   ctx.init_plugins()?;
//!
//!   // Optionally set the log output format.
//!   ctx.set_log_format("bar")?;
//!
//!   let store = Arc::new(Store::open(&ctx, None)?);
//!   let _state = EvalStateBuilder::new(&store)?.build()?;
//!   Ok(())
//! }
//! ```

use std::ffi::CString;

use crate::{Context, Result, check_err, sys};

impl Context {
  /// Load the plugins listed in Nix's `plugin-files` setting.
  ///
  /// This must be called **after** all [`set_setting`](Context::set_setting)
  /// calls and **before** creating an
  /// [`EvalState`](crate::EvalState). Calling it multiple times is safe but
  /// redundant.
  ///
  /// # Errors
  ///
  /// Returns an error if plugin loading fails (e.g. a plugin file is not
  /// found or fails to initialize).
  pub fn init_plugins(&self) -> Result<()> {
    // SAFETY: context is valid; nix_init_plugins reads the global settings
    // that have already been set and loads shared libraries accordingly.
    unsafe {
      check_err(
        self.inner.as_ptr(),
        sys::nix_init_plugins(self.inner.as_ptr()),
      )
    }
  }

  /// Set the log output format.
  ///
  /// Must be called before any log output is produced. Valid format names
  /// match those accepted by Nix's `--log-format` flag:
  ///
  /// | Name                    | Description                      |
  /// | ----------------------- | -------------------------------- |
  /// | `"raw"`                 | Plain text, no decoration        |
  /// | `"internal-json"`       | Machine-readable JSON            |
  /// | `"bar"`                 | Progress bar                     |
  /// | `"bar-with-logs"`       | Progress bar with log lines      |
  /// | `"multiline"`           | Multi-line output                |
  /// | `"multiline-with-logs"` | Multi-line output with log lines |
  ///
  /// # Errors
  ///
  /// Returns an error if `format` is not a recognised log-format name or
  /// contains an interior NUL byte.
  pub fn set_log_format(&self, format: &str) -> Result<()> {
    let format_c = CString::new(format)?;
    // SAFETY: context and format_c are valid; nix_set_log_format copies the
    // string so the CString lifetime does not need to outlive the call.
    unsafe {
      check_err(
        self.inner.as_ptr(),
        sys::nix_set_log_format(self.inner.as_ptr(), format_c.as_ptr()),
      )
    }
  }
}
