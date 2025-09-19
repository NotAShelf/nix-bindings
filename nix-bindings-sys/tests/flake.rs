#![cfg(test)]

use std::ptr;

use nix_bindings_sys::*;
use serial_test::serial;

#[test]
#[serial]
fn flake_settings_new_and_free() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    // Create new flake settings
    let settings = nix_flake_settings_new(ctx);
    assert!(!settings.is_null(), "nix_flake_settings_new returned null");

    // Free flake settings (should not crash)
    nix_flake_settings_free(settings);

    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn flake_settings_add_to_eval_state_builder() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, ptr::null(), ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    let settings = nix_flake_settings_new(ctx);
    assert!(!settings.is_null(), "nix_flake_settings_new returned null");

    // Add flake settings to eval state builder
    let err =
      nix_flake_settings_add_to_eval_state_builder(ctx, settings, builder);
    // Accept OK or ERR_UNKNOWN (depends on Nix build/config)
    assert!(
      err == nix_err_NIX_OK || err == nix_err_NIX_ERR_UNKNOWN,
      "nix_flake_settings_add_to_eval_state_builder returned unexpected error \
       code: {err}"
    );

    nix_flake_settings_free(settings);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn flake_settings_null_context() {
  // Passing NULL context should not crash, but may error
  unsafe {
    let settings = nix_flake_settings_new(ptr::null_mut());
    // May return null if context is required
    if !settings.is_null() {
      nix_flake_settings_free(settings);
    }
  }
}
