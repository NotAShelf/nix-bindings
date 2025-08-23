#![cfg(test)]

use nix_bindings::*;
use serial_test::serial;
use std::ffi::CString;

#[test]
#[serial]
fn eval_init_and_state_build() {
    unsafe {
        let ctx = nix_c_context_create();
        assert!(!ctx.is_null());
        let err = nix_libutil_init(ctx);
        assert_eq!(err, nix_err_NIX_OK, "nix_libutil_init failed: {}", err);

        let err = nix_libstore_init(ctx);
        assert_eq!(err, nix_err_NIX_OK, "nix_libstore_init failed: {}", err);

        let err = nix_libexpr_init(ctx);
        assert_eq!(err, nix_err_NIX_OK, "nix_libexpr_init failed: {}", err);

        let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
        assert!(!store.is_null());

        let builder = nix_eval_state_builder_new(ctx, store);
        assert!(!builder.is_null());

        let load_err = nix_eval_state_builder_load(ctx, builder);
        assert_eq!(load_err, nix_err_NIX_OK);

        let state = nix_eval_state_build(ctx, builder);
        assert!(!state.is_null());

        nix_state_free(state);
        nix_eval_state_builder_free(builder);
        nix_store_free(store);
        nix_c_context_free(ctx);
    }
}

#[test]
#[serial]
fn eval_simple_expression() {
    unsafe {
        let ctx = nix_c_context_create();
        assert!(!ctx.is_null());

        let err = nix_libutil_init(ctx);
        assert_eq!(err, nix_err_NIX_OK, "nix_libutil_init failed: {}", err);

        let err = nix_libstore_init(ctx);
        assert_eq!(err, nix_err_NIX_OK, "nix_libstore_init failed: {}", err);

        let err = nix_libexpr_init(ctx);
        assert_eq!(err, nix_err_NIX_OK, "nix_libexpr_init failed: {}", err);

        let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
        assert!(!store.is_null());

        let builder = nix_eval_state_builder_new(ctx, store);
        assert!(!builder.is_null());
        assert_eq!(nix_eval_state_builder_load(ctx, builder), nix_err_NIX_OK);

        let state = nix_eval_state_build(ctx, builder);
        assert!(!state.is_null());

        // Evaluate a simple integer expression
        let expr = CString::new("1 + 2").unwrap();
        let path = CString::new("<eval>").unwrap();
        let mut value = std::mem::MaybeUninit::<nix_value>::uninit();

        let eval_err =
            nix_expr_eval_from_string(ctx, state, expr.as_ptr(), path.as_ptr(), value.as_mut_ptr());
        assert_eq!(eval_err, nix_err_NIX_OK);

        // Force the value (should not be a thunk)
        let force_err = nix_value_force(ctx, state, value.as_mut_ptr());
        assert_eq!(force_err, nix_err_NIX_OK);

        nix_state_free(state);
        nix_eval_state_builder_free(builder);
        nix_store_free(store);
        nix_c_context_free(ctx);
    }
}
