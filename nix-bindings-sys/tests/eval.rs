#![cfg(test)]

use std::{
  ffi::{CStr, CString},
  mem::MaybeUninit,
  ptr,
};

use nix_bindings_sys::*;
use serial_test::serial;

#[test]
#[serial]
fn eval_init_and_state_build() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK, "nix_libutil_init failed: {err}");

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK, "nix_libstore_init failed: {err}");

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK, "nix_libexpr_init failed: {err}");

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
    assert_eq!(err, nix_err_NIX_OK, "nix_libutil_init failed: {err}");

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK, "nix_libstore_init failed: {err}");

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK, "nix_libexpr_init failed: {err}");

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

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      value.as_mut_ptr(),
    );
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

#[test]
#[serial]
fn value_construction_and_inspection() {
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
    assert_eq!(nix_eval_state_builder_load(ctx, builder), nix_err_NIX_OK);
    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Int
    let int_val = nix_alloc_value(ctx, state);
    assert!(!int_val.is_null());
    assert_eq!(nix_init_int(ctx, int_val, 42), nix_err_NIX_OK);
    assert_eq!(nix_get_type(ctx, int_val), ValueType_NIX_TYPE_INT);
    assert_eq!(nix_get_int(ctx, int_val), 42);

    // Float
    let float_val = nix_alloc_value(ctx, state);
    assert!(!float_val.is_null());
    assert_eq!(
      nix_init_float(ctx, float_val, std::f64::consts::PI),
      nix_err_NIX_OK
    );
    assert_eq!(nix_get_type(ctx, float_val), ValueType_NIX_TYPE_FLOAT);
    assert!(
      (nix_get_float(ctx, float_val) - std::f64::consts::PI).abs() < 1e-10
    );

    // Bool
    let bool_val = nix_alloc_value(ctx, state);
    assert!(!bool_val.is_null());
    assert_eq!(nix_init_bool(ctx, bool_val, true), nix_err_NIX_OK);
    assert_eq!(nix_get_type(ctx, bool_val), ValueType_NIX_TYPE_BOOL);
    assert!(nix_get_bool(ctx, bool_val));

    // Null
    let null_val = nix_alloc_value(ctx, state);
    assert!(!null_val.is_null());
    assert_eq!(nix_init_null(ctx, null_val), nix_err_NIX_OK);
    assert_eq!(nix_get_type(ctx, null_val), ValueType_NIX_TYPE_NULL);

    // String
    let string_val = nix_alloc_value(ctx, state);
    assert!(!string_val.is_null());
    let s = CString::new("hello world").unwrap();
    assert_eq!(nix_init_string(ctx, string_val, s.as_ptr()), nix_err_NIX_OK);
    assert_eq!(nix_get_type(ctx, string_val), ValueType_NIX_TYPE_STRING);
    extern "C" fn string_cb(
      start: *const ::std::os::raw::c_char,
      n: ::std::os::raw::c_uint,
      user_data: *mut ::std::os::raw::c_void,
    ) {
      let s =
        unsafe { std::slice::from_raw_parts(start.cast::<u8>(), n as usize) };
      let s = std::str::from_utf8(s).unwrap();
      let out = user_data.cast::<Option<String>>();
      unsafe { *out = Some(s.to_string()) };
    }
    let mut got: Option<String> = None;
    assert_eq!(
      nix_get_string(ctx, string_val, Some(string_cb), (&raw mut got).cast()),
      nix_err_NIX_OK
    );
    assert_eq!(got.as_deref(), Some("hello world"));

    // Path string
    let path_val = nix_alloc_value(ctx, state);
    assert!(!path_val.is_null());
    let p = CString::new("/nix/store/foo").unwrap();
    assert_eq!(
      nix_init_path_string(ctx, state, path_val, p.as_ptr()),
      nix_err_NIX_OK
    );
    assert_eq!(nix_get_type(ctx, path_val), ValueType_NIX_TYPE_PATH);
    let path_ptr = nix_get_path_string(ctx, path_val);
    assert!(!path_ptr.is_null());
    let path_str = CStr::from_ptr(path_ptr).to_string_lossy();
    assert_eq!(path_str, "/nix/store/foo");

    // Clean up
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn list_and_attrset_manipulation() {
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
    assert_eq!(nix_eval_state_builder_load(ctx, builder), nix_err_NIX_OK);
    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // List: [1, 2, 3]
    let list_builder = nix_make_list_builder(ctx, state, 3);
    assert!(!list_builder.is_null());
    let v1 = nix_alloc_value(ctx, state);
    let v2 = nix_alloc_value(ctx, state);
    let v3 = nix_alloc_value(ctx, state);
    nix_init_int(ctx, v1, 1);
    nix_init_int(ctx, v2, 2);
    nix_init_int(ctx, v3, 3);
    nix_list_builder_insert(ctx, list_builder, 0, v1);
    nix_list_builder_insert(ctx, list_builder, 1, v2);
    nix_list_builder_insert(ctx, list_builder, 2, v3);

    let list_val = nix_alloc_value(ctx, state);
    assert_eq!(nix_make_list(ctx, list_builder, list_val), nix_err_NIX_OK);
    assert_eq!(nix_get_type(ctx, list_val), ValueType_NIX_TYPE_LIST);
    assert_eq!(nix_get_list_size(ctx, list_val), 3);

    // Get elements by index
    for i in 0..3 {
      let elem = nix_get_list_byidx(ctx, list_val, state, i);
      assert!(!elem.is_null());
      assert_eq!(nix_get_type(ctx, elem), ValueType_NIX_TYPE_INT);
      assert_eq!(nix_get_int(ctx, elem), i64::from(i + 1));
    }

    nix_list_builder_free(list_builder);

    // Attrset: { foo = 42; bar = "baz"; }
    let attr_builder = nix_make_bindings_builder(ctx, state, 2);
    assert!(!attr_builder.is_null());
    let foo_val = nix_alloc_value(ctx, state);
    let bar_val = nix_alloc_value(ctx, state);
    nix_init_int(ctx, foo_val, 42);
    let baz = CString::new("baz").unwrap();
    nix_init_string(ctx, bar_val, baz.as_ptr());
    let foo = CString::new("foo").unwrap();
    let bar = CString::new("bar").unwrap();
    nix_bindings_builder_insert(ctx, attr_builder, foo.as_ptr(), foo_val);
    nix_bindings_builder_insert(ctx, attr_builder, bar.as_ptr(), bar_val);

    let attr_val = nix_alloc_value(ctx, state);
    assert_eq!(nix_make_attrs(ctx, attr_val, attr_builder), nix_err_NIX_OK);
    assert_eq!(nix_get_type(ctx, attr_val), ValueType_NIX_TYPE_ATTRS);
    assert_eq!(nix_get_attrs_size(ctx, attr_val), 2);

    // Get by name
    let foo_got = nix_get_attr_byname(ctx, attr_val, state, foo.as_ptr());
    assert!(!foo_got.is_null());
    assert_eq!(nix_get_type(ctx, foo_got), ValueType_NIX_TYPE_INT);
    assert_eq!(nix_get_int(ctx, foo_got), 42);

    let bar_got = nix_get_attr_byname(ctx, attr_val, state, bar.as_ptr());
    assert!(!bar_got.is_null());
    assert_eq!(nix_get_type(ctx, bar_got), ValueType_NIX_TYPE_STRING);

    // Has attr
    assert!(nix_has_attr_byname(ctx, attr_val, state, foo.as_ptr()));
    assert!(nix_has_attr_byname(ctx, attr_val, state, bar.as_ptr()));

    nix_bindings_builder_free(attr_builder);

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn function_application_and_force() {
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
    assert_eq!(nix_eval_state_builder_load(ctx, builder), nix_err_NIX_OK);
    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Evaluate a function and apply it: (x: x + 1) 41
    let expr = CString::new("(x: x + 1)").unwrap();
    let path = CString::new("<eval>").unwrap();
    let mut fn_val = MaybeUninit::<nix_value>::uninit();
    assert_eq!(
      nix_expr_eval_from_string(
        ctx,
        state,
        expr.as_ptr(),
        path.as_ptr(),
        fn_val.as_mut_ptr()
      ),
      nix_err_NIX_OK
    );

    // Argument: 41
    let arg_val = nix_alloc_value(ctx, state);
    nix_init_int(ctx, arg_val, 41);

    // Result value
    let mut result_val = MaybeUninit::<nix_value>::uninit();
    assert_eq!(
      nix_value_call(
        ctx,
        state,
        fn_val.as_mut_ptr(),
        arg_val,
        result_val.as_mut_ptr()
      ),
      nix_err_NIX_OK
    );

    // Force result
    assert_eq!(
      nix_value_force(ctx, state, result_val.as_mut_ptr()),
      nix_err_NIX_OK
    );
    assert_eq!(
      nix_get_type(ctx, result_val.as_mut_ptr()),
      ValueType_NIX_TYPE_INT
    );
    assert_eq!(nix_get_int(ctx, result_val.as_mut_ptr()), 42);

    // Deep force (should be a no-op for int)
    assert_eq!(
      nix_value_force_deep(ctx, state, result_val.as_mut_ptr()),
      nix_err_NIX_OK
    );

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn error_handling_invalid_expression() {
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
    assert_eq!(nix_eval_state_builder_load(ctx, builder), nix_err_NIX_OK);
    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Invalid expression
    let expr = CString::new("this is not valid nix").unwrap();
    let path = CString::new("<eval>").unwrap();
    let mut value = MaybeUninit::<nix_value>::uninit();
    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      value.as_mut_ptr(),
    );
    assert_ne!(eval_err, nix_err_NIX_OK);

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn realised_string_and_gc() {
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
    assert_eq!(nix_eval_state_builder_load(ctx, builder), nix_err_NIX_OK);
    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // String value
    let string_val = nix_alloc_value(ctx, state);
    let s = CString::new("hello world").unwrap();
    assert_eq!(nix_init_string(ctx, string_val, s.as_ptr()), nix_err_NIX_OK);

    // Realise string
    let realised = nix_string_realise(ctx, state, string_val, false);
    assert!(!realised.is_null());
    let buf = nix_realised_string_get_buffer_start(realised);
    let len = nix_realised_string_get_buffer_size(realised);
    let realised_str =
      std::str::from_utf8(std::slice::from_raw_parts(buf.cast::<u8>(), len))
        .unwrap();
    assert_eq!(realised_str, "hello world");
    assert_eq!(nix_realised_string_get_store_path_count(realised), 0);

    nix_realised_string_free(realised);

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
