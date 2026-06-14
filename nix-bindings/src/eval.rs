//! [`EvalState`] and [`EvalStateBuilder`]: the Nix evaluator handle and
//! its configuration.

#![cfg(feature = "expr")]

use std::{ffi::CString, path::Path, ptr::NonNull, sync::Arc};

use crate::{
  Context,
  Error,
  Result,
  Store,
  StorePath,
  Value,
  context::is_pure_eval,
  error::check_err,
  sys,
};

/// Builder for Nix evaluation state.
///
/// This allows configuring the evaluation environment before creating
/// the evaluation state.
pub struct EvalStateBuilder {
  inner:     NonNull<sys::nix_eval_state_builder>,
  store:     Arc<Store>,
  context:   Arc<Context>,
  skip_load: bool,
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
      skip_load: false,
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
    let c_strings: Vec<CString> = paths
      .iter()
      .map(|s| CString::new(s.as_ref()))
      .collect::<std::result::Result<_, _>>()?;

    let mut ptrs: Vec<*const std::os::raw::c_char> =
      c_strings.iter().map(|cs| cs.as_ptr()).collect();
    ptrs.push(std::ptr::null());

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
  #[cfg(feature = "flake")]
  pub fn with_flake_settings(
    self,
    settings: &crate::flake::FlakeSettings,
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

  /// Skip loading Nix configuration from the environment.
  ///
  /// By default [`build`](Self::build) calls `nix_eval_state_builder_load` to
  /// read configuration from environment variables and config files. Call
  /// this method to skip that step, which is useful in tests or sandboxed
  /// environments.
  #[must_use]
  pub fn no_load_config(mut self) -> Self {
    self.skip_load = true;
    self
  }

  /// Build the evaluation state.
  ///
  /// # Errors
  ///
  /// Returns an error if the evaluation state cannot be built.
  pub fn build(self) -> Result<EvalState> {
    if !self.skip_load {
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
    }

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
  #[expect(dead_code, reason = "keeps the Arc<Store> alive Drop side-effects")]
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

    // SAFETY: context and state are valid
    let value_ptr = unsafe {
      sys::nix_alloc_value(self.context.as_ptr(), self.inner.as_ptr())
    };
    if value_ptr.is_null() {
      return Err(Error::NullPointer);
    }

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

  /// Evaluate a Nix expression from a file.
  ///
  /// Reads the file at `path` as UTF-8, then evaluates its contents using the
  /// parent directory as the base path for relative imports. The base path is
  /// passed to Nix as a UTF-8 string; non-UTF-8 components are replaced
  /// lossily for the error-reporting label only.
  ///
  /// # Errors
  ///
  /// Returns an error if the file cannot be read as UTF-8 or if evaluation
  /// fails.
  pub fn eval_from_file(&self, path: impl AsRef<Path>) -> Result<Value<'_>> {
    let path = path.as_ref();
    let expr = std::fs::read_to_string(path).map_err(|e| {
      Error::Unknown(format!("Failed to read file {}: {e}", path.display()))
    })?;
    let base_path = path.parent().unwrap_or_else(|| Path::new("."));
    let base_str = base_path.to_string_lossy();
    self.eval_from_string(&expr, &base_str)
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
  /// # Pure Evaluation
  ///
  /// In pure-eval mode (`--pure-eval`) the Nix evaluator wraps the
  /// filesystem in an `AllowListSourceAccessor` that rejects any
  /// unregistered absolute path. When the `shim` feature is enabled this
  /// method automatically registers absolute paths via the shim's
  /// `nix_eval_state_allow_path` before constructing the value, mirroring
  /// what Nix's own fetch builtins do. Without `shim` you must arrange
  /// for allowPath yourself, or [`is_pure_eval`] returns true.
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

    // See make_path note in old lib.rs for why we restrict auto-allow
    // to /nix/store/ paths.
    #[cfg(feature = "shim")]
    if path.as_ref().is_absolute()
      && path_str.starts_with("/nix/store/")
      && is_pure_eval()
    {
      // SAFETY: context, state, and path are valid.
      unsafe {
        check_err(
          self.context.as_ptr(),
          sys::nix_eval_state_allow_path(
            self.context.as_ptr(),
            self.inner.as_ptr(),
            path_c.as_ptr(),
          ),
        )?;
      }
    }

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

    struct ListBuilderGuard(*mut sys::ListBuilder);
    impl Drop for ListBuilderGuard {
      fn drop(&mut self) {
        unsafe { sys::nix_list_builder_free(self.0) };
      }
    }
    let _guard = ListBuilderGuard(builder);

    for (i, item) in items.iter().enumerate() {
      // SAFETY: context, builder, and value are valid; index in bounds
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
  /// Returns an error if value allocation or attribute set construction
  /// fails.
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

    struct BindingsBuilderGuard(*mut sys::BindingsBuilder);
    impl Drop for BindingsBuilderGuard {
      fn drop(&mut self) {
        unsafe { sys::nix_bindings_builder_free(self.0) };
      }
    }
    let _guard = BindingsBuilderGuard(builder);

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

  /// Determine whether a value is a derivation and return its store path.
  ///
  /// Forces `value` and, if it is a derivation, returns a newly allocated
  /// [`StorePath`] for its `.drvPath`. Returns `Ok(None)` when the value is
  /// not a derivation. An exception raised during forcing propagates as
  /// `Err(...)`.
  ///
  /// # Errors
  ///
  /// Returns an error if forcing the value raises a Nix exception.
  #[cfg(feature = "shim")]
  pub fn get_derivation(&self, value: &Value<'_>) -> Result<Option<StorePath>> {
    // SAFETY: context, state, and value are valid for the call duration.
    let path_ptr = unsafe {
      sys::nix_get_derivation(
        self.context.as_ptr(),
        self.inner.as_ptr(),
        value.inner.as_ptr(),
        false,
      )
    };

    if path_ptr.is_null() {
      // NIXC_CATCH_ERRS_NULL sets last_err_code on exception. A plain null
      // (no exception, maybePkg was empty) leaves last_err_code at NIX_OK.
      // SAFETY: context is valid for the lifetime of self.
      unsafe {
        let ctx = self.context.as_ptr();
        let code = sys::nix_err_code(ctx);
        check_err(ctx, code)?;
      }
      return Ok(None);
    }

    let inner = NonNull::new(path_ptr).ok_or(Error::NullPointer)?;
    Ok(Some(StorePath {
      inner,
      _context: Arc::clone(&self.context),
    }))
  }

  /// Call a function using an attribute set as its argument source.
  ///
  /// Forces `fn_val` and writes the result into a newly allocated value:
  ///
  /// - If `fn_val` is a function with named formals, each formal is looked up
  ///   in `auto_args`; formals with defaults that are absent from `auto_args`
  ///   use their defaults.
  /// - If `auto_args` is `None`, empty bindings are supplied (every formal must
  ///   then have a default).
  /// - If `fn_val` is not a function, the value is copied to the result
  ///   unchanged.
  ///
  /// # Errors
  ///
  /// Returns an error if the call fails.
  #[cfg(feature = "shim")]
  pub fn auto_call_function<'s>(
    &'s self,
    auto_args: Option<&Value<'_>>,
    fn_val: &Value<'_>,
  ) -> Result<Value<'s>> {
    let result = self.alloc_value()?;
    let auto_args_ptr =
      auto_args.map_or(std::ptr::null_mut(), |v| v.inner.as_ptr());

    // SAFETY: context, state, auto_args_ptr (null or valid), fn_val, and
    // result are all valid for the call duration.
    unsafe {
      check_err(
        self.context.as_ptr(),
        sys::nix_value_auto_call_function(
          self.context.as_ptr(),
          self.inner.as_ptr(),
          auto_args_ptr,
          fn_val.inner.as_ptr(),
          result.inner.as_ptr(),
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

// SAFETY: see crate-level "# Thread Safety" docs and the comment in
// the original lib.rs Send impl.
unsafe impl Send for EvalState {}
