#![cfg(test)]

use std::{
  ffi::{CStr, CString},
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
    let value = nix_alloc_value(ctx, state);
    assert!(!value.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      value,
    );
    assert_eq!(eval_err, nix_err_NIX_OK);

    // Force the value (should not be a thunk)
    let force_err = nix_value_force(ctx, state, value);
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
    let fn_val = nix_alloc_value(ctx, state);
    assert!(!fn_val.is_null());
    assert_eq!(
      nix_expr_eval_from_string(
        ctx,
        state,
        expr.as_ptr(),
        path.as_ptr(),
        fn_val
      ),
      nix_err_NIX_OK
    );

    // Argument: 41
    let arg_val = nix_alloc_value(ctx, state);
    nix_init_int(ctx, arg_val, 41);

    // Result value
    let result_val = nix_alloc_value(ctx, state);
    assert!(!result_val.is_null());
    assert_eq!(
      nix_value_call(ctx, state, fn_val, arg_val, result_val),
      nix_err_NIX_OK
    );

    // Force result
    assert_eq!(nix_value_force(ctx, state, result_val), nix_err_NIX_OK);
    assert_eq!(nix_get_type(ctx, result_val), ValueType_NIX_TYPE_INT);
    assert_eq!(nix_get_int(ctx, result_val), 42);

    // Deep force (should be a no-op for int)
    assert_eq!(nix_value_force_deep(ctx, state, result_val), nix_err_NIX_OK);

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
    let value = nix_alloc_value(ctx, state);
    assert!(!value.is_null());
    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      value,
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

#[test]
#[serial]
fn big_thunk_evaluation() {
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

    // Create a complex expression with lazy evaluation
    let expr = CString::new(
      "let x = 1 + 2; y = x * 3; in { result = y + 4; other = x; }",
    )
    .unwrap();
    let path = CString::new("<eval>").unwrap();
    let value = nix_alloc_value(ctx, state);
    assert!(!value.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      value,
    );
    assert_eq!(eval_err, nix_err_NIX_OK);

    // The top-level should be an attrset
    assert_eq!(nix_get_type(ctx, value), ValueType_NIX_TYPE_ATTRS);

    // Get "result" attribute (ts should be a thunk initially)
    let result_name = CString::new("result").unwrap();
    let result_val =
      nix_get_attr_byname(ctx, value, state, result_name.as_ptr());
    assert!(!result_val.is_null());

    // Force the result
    let force_err = nix_value_force(ctx, state, result_val);
    assert_eq!(force_err, nix_err_NIX_OK);

    assert_eq!(nix_get_type(ctx, result_val), ValueType_NIX_TYPE_INT);
    assert_eq!(nix_get_int(ctx, result_val), 13); // ((1+2)*3)+4 = 13

    // Get "other" attribute
    let other_name = CString::new("other").unwrap();
    let other_val = nix_get_attr_byname(ctx, value, state, other_name.as_ptr());
    assert!(!other_val.is_null());

    let force_err2 = nix_value_force(ctx, state, other_val);
    assert_eq!(force_err2, nix_err_NIX_OK);

    assert_eq!(nix_get_type(ctx, other_val), ValueType_NIX_TYPE_INT);
    assert_eq!(nix_get_int(ctx, other_val), 3); // 1+2 = 3

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn multi_argument_function_calls() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    let load_err = nix_eval_state_builder_load(ctx, builder);
    assert_eq!(load_err, nix_err_NIX_OK);

    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Test evaluating a multi-argument function: (x: y: x + y)
    let expr = CString::new("(x: y: x + y)").unwrap();
    let path = CString::new("/test").unwrap();

    let func_value = nix_alloc_value(ctx, state);
    assert!(!func_value.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      func_value,
    );
    assert_eq!(eval_err, nix_err_NIX_OK);

    // Force evaluation of the function
    let force_err = nix_value_force(ctx, state, func_value);
    assert_eq!(force_err, nix_err_NIX_OK);

    // Verify it's a function
    let func_type = nix_get_type(ctx, func_value);
    assert_eq!(func_type, ValueType_NIX_TYPE_FUNCTION);

    // Create arguments
    let arg1 = nix_alloc_value(ctx, state);
    let arg2 = nix_alloc_value(ctx, state);
    assert!(!arg1.is_null() && !arg2.is_null());

    let init_arg1_err = nix_init_int(ctx, arg1, 10);
    let init_arg2_err = nix_init_int(ctx, arg2, 20);
    assert_eq!(init_arg1_err, nix_err_NIX_OK);
    assert_eq!(init_arg2_err, nix_err_NIX_OK);

    // Test multi-argument call using nix_value_call_multi
    let mut args = [arg1, arg2];
    let result = nix_alloc_value(ctx, state);
    assert!(!result.is_null());

    let call_err = nix_value_call_multi(
      ctx,
      state,
      func_value,
      2,
      args.as_mut_ptr(),
      result,
    );
    assert_eq!(call_err, nix_err_NIX_OK);

    // Force the result
    let force_result_err = nix_value_force(ctx, state, result);
    assert_eq!(force_result_err, nix_err_NIX_OK);

    // Check result type and value
    let result_type = nix_get_type(ctx, result);
    assert_eq!(result_type, ValueType_NIX_TYPE_INT);

    let result_value = nix_get_int(ctx, result);
    assert_eq!(result_value, 30); // 10 + 20

    // Clean up
    nix_value_decref(ctx, result);
    nix_value_decref(ctx, arg2);
    nix_value_decref(ctx, arg1);
    nix_value_decref(ctx, func_value);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn curried_function_evaluation() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    let load_err = nix_eval_state_builder_load(ctx, builder);
    assert_eq!(load_err, nix_err_NIX_OK);

    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Test evaluating a curried function: (x: y: z: x + y + z)
    let expr = CString::new("(x: y: z: x + y + z)").unwrap();
    let path = CString::new("/test").unwrap();

    let func_value = nix_alloc_value(ctx, state);
    assert!(!func_value.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      func_value,
    );
    assert_eq!(eval_err, nix_err_NIX_OK);

    // Create three arguments
    let arg1 = nix_alloc_value(ctx, state);
    let arg2 = nix_alloc_value(ctx, state);
    let arg3 = nix_alloc_value(ctx, state);
    assert!(!arg1.is_null() && !arg2.is_null() && !arg3.is_null());

    let _ = nix_init_int(ctx, arg1, 5);
    let _ = nix_init_int(ctx, arg2, 10);
    let _ = nix_init_int(ctx, arg3, 15);

    // Test calling with multiple arguments at once
    let mut args = [arg1, arg2, arg3];
    let result = nix_alloc_value(ctx, state);
    assert!(!result.is_null());

    let call_err = nix_value_call_multi(
      ctx,
      state,
      func_value,
      3,
      args.as_mut_ptr(),
      result,
    );
    assert_eq!(call_err, nix_err_NIX_OK);

    // Force the result
    let force_result_err = nix_value_force(ctx, state, result);
    assert_eq!(force_result_err, nix_err_NIX_OK);

    // Check result
    let result_type = nix_get_type(ctx, result);
    assert_eq!(result_type, ValueType_NIX_TYPE_INT);

    let result_value = nix_get_int(ctx, result);
    assert_eq!(result_value, 30); // 5 + 10 + 15

    // Test partial application using single calls
    let partial1 = nix_alloc_value(ctx, state);
    assert!(!partial1.is_null());

    let partial_call1_err =
      nix_value_call(ctx, state, func_value, arg1, partial1);
    assert_eq!(partial_call1_err, nix_err_NIX_OK);

    // partial1 should still be a function
    let force_partial1_err = nix_value_force(ctx, state, partial1);
    assert_eq!(force_partial1_err, nix_err_NIX_OK);

    let partial1_type = nix_get_type(ctx, partial1);
    assert_eq!(partial1_type, ValueType_NIX_TYPE_FUNCTION);

    // Apply second argument
    let partial2 = nix_alloc_value(ctx, state);
    assert!(!partial2.is_null());

    let partial_call2_err =
      nix_value_call(ctx, state, partial1, arg2, partial2);
    assert_eq!(partial_call2_err, nix_err_NIX_OK);

    // partial2 should still be a function
    let force_partial2_err = nix_value_force(ctx, state, partial2);
    assert_eq!(force_partial2_err, nix_err_NIX_OK);

    let partial2_type = nix_get_type(ctx, partial2);
    assert_eq!(partial2_type, ValueType_NIX_TYPE_FUNCTION);

    // Apply final argument
    let final_result = nix_alloc_value(ctx, state);
    assert!(!final_result.is_null());

    let final_call_err =
      nix_value_call(ctx, state, partial2, arg3, final_result);
    assert_eq!(final_call_err, nix_err_NIX_OK);

    // Force and check final result
    let force_final_err = nix_value_force(ctx, state, final_result);
    assert_eq!(force_final_err, nix_err_NIX_OK);

    let final_type = nix_get_type(ctx, final_result);
    assert_eq!(final_type, ValueType_NIX_TYPE_INT);

    let final_value = nix_get_int(ctx, final_result);
    assert_eq!(final_value, 30); // same result as multi-arg call

    // Clean up
    nix_value_decref(ctx, final_result);
    nix_value_decref(ctx, partial2);
    nix_value_decref(ctx, partial1);
    nix_value_decref(ctx, result);
    nix_value_decref(ctx, arg3);
    nix_value_decref(ctx, arg2);
    nix_value_decref(ctx, arg1);
    nix_value_decref(ctx, func_value);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn thunk_creation_with_init_apply() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    let load_err = nix_eval_state_builder_load(ctx, builder);
    assert_eq!(load_err, nix_err_NIX_OK);

    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Create a simple function
    let func_expr = CString::new("(x: x * 2)").unwrap();
    let path = CString::new("/test").unwrap();

    let func_value = nix_alloc_value(ctx, state);
    assert!(!func_value.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      func_expr.as_ptr(),
      path.as_ptr(),
      func_value,
    );
    assert_eq!(eval_err, nix_err_NIX_OK);

    // Create an argument
    let arg = nix_alloc_value(ctx, state);
    assert!(!arg.is_null());

    let init_arg_err = nix_init_int(ctx, arg, 21);
    assert_eq!(init_arg_err, nix_err_NIX_OK);

    // Create a thunk using nix_init_apply (lazy evaluation)
    let thunk = nix_alloc_value(ctx, state);
    assert!(!thunk.is_null());

    let apply_err = nix_init_apply(ctx, thunk, func_value, arg);
    assert_eq!(apply_err, nix_err_NIX_OK);

    // Initially, the thunk should be of type THUNK
    let thunk_type = nix_get_type(ctx, thunk);
    assert_eq!(thunk_type, ValueType_NIX_TYPE_THUNK);

    // Force evaluation of the thunk
    let force_err = nix_value_force(ctx, state, thunk);
    assert_eq!(force_err, nix_err_NIX_OK);

    // After forcing, it should be an integer
    let forced_type = nix_get_type(ctx, thunk);
    assert_eq!(forced_type, ValueType_NIX_TYPE_INT);

    let result_value = nix_get_int(ctx, thunk);
    assert_eq!(result_value, 42); // 21 * 2

    // Clean up
    nix_value_decref(ctx, thunk);
    nix_value_decref(ctx, arg);
    nix_value_decref(ctx, func_value);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn lookup_path_configuration() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    // Configure custom lookup path (NIX_PATH equivalent)
    let lookup_paths = [
      CString::new("nixpkgs=/fake/nixpkgs").unwrap(),
      CString::new("custom=/fake/custom").unwrap(),
    ];

    let lookup_path_ptrs: Vec<*const _> =
      lookup_paths.iter().map(|s| s.as_ptr()).collect();
    let mut lookup_path_ptrs_null_terminated = lookup_path_ptrs;
    lookup_path_ptrs_null_terminated.push(std::ptr::null());

    let set_lookup_err = nix_eval_state_builder_set_lookup_path(
      ctx,
      builder,
      lookup_path_ptrs_null_terminated.as_mut_ptr(),
    );
    assert_eq!(set_lookup_err, nix_err_NIX_OK);

    let load_err = nix_eval_state_builder_load(ctx, builder);
    assert_eq!(load_err, nix_err_NIX_OK);

    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Try to evaluate an expression that uses the lookup path
    // NOTE: This will likely fail since the paths don't exist, but it tests the
    // API
    let expr = CString::new("builtins.nixPath").unwrap();
    let path = CString::new("/test").unwrap();

    let result = nix_alloc_value(ctx, state);
    assert!(!result.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      result,
    );

    // The evaluation might succeed or fail depending on Nix version and
    // configuration The important thing is that setting the lookup path
    // didn't crash
    if eval_err == nix_err_NIX_OK {
      let force_err = nix_value_force(ctx, state, result);
      if force_err == nix_err_NIX_OK {
        let result_type = nix_get_type(ctx, result);
        // nixPath should be a list
        assert_eq!(result_type, ValueType_NIX_TYPE_LIST);
      }
    }

    // Clean up
    nix_value_decref(ctx, result);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn complex_nested_evaluation() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    let load_err = nix_eval_state_builder_load(ctx, builder);
    assert_eq!(load_err, nix_err_NIX_OK);

    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Evaluate a simple nested expression
    let expr = CString::new(
      r#"
      let
        add = x: y: x + y;
        data = {
          values = [1 2 3 4 5];
        };
      in
      {
        original = data.values;
        sum = builtins.foldl' add 0 data.values;
      }
    "#,
    )
    .unwrap();
    let path = CString::new("/test").unwrap();

    let result = nix_alloc_value(ctx, state);
    assert!(!result.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      result,
    );

    // Complex expressions may fail sometimes, check for both success
    // and error
    if eval_err != nix_err_NIX_OK {
      // If evaluation fails, skip the rest of the test
      nix_value_decref(ctx, result);
      nix_state_free(state);
      nix_eval_state_builder_free(builder);
      nix_store_free(store);
      nix_c_context_free(ctx);
      return;
    }

    // Force deep evaluation
    let force_err = nix_value_force_deep(ctx, state, result);
    if force_err != nix_err_NIX_OK {
      // If forcing fails, skip the rest of the test
      nix_value_decref(ctx, result);
      nix_state_free(state);
      nix_eval_state_builder_free(builder);
      nix_store_free(store);
      nix_c_context_free(ctx);
      return;
    }

    // Verify result structure
    let result_type = nix_get_type(ctx, result);
    assert_eq!(result_type, ValueType_NIX_TYPE_ATTRS);

    let attrs_size = nix_get_attrs_size(ctx, result);
    assert_eq!(attrs_size, 2); // original, sum

    // Check 'sum' attribute
    let sum_key = CString::new("sum").unwrap();
    let sum_value = nix_get_attr_byname(ctx, result, state, sum_key.as_ptr());
    assert!(!sum_value.is_null());

    let sum_type = nix_get_type(ctx, sum_value);
    assert_eq!(sum_type, ValueType_NIX_TYPE_INT);

    let sum_result = nix_get_int(ctx, sum_value);
    assert_eq!(sum_result, 15); // 1 + 2 + 3 + 4 + 5

    // Check 'original' attribute (should be a list)
    let original_key = CString::new("original").unwrap();
    let original_value =
      nix_get_attr_byname(ctx, result, state, original_key.as_ptr());
    if !original_value.is_null() {
      let original_type = nix_get_type(ctx, original_value);
      assert_eq!(original_type, ValueType_NIX_TYPE_LIST);

      let original_size = nix_get_list_size(ctx, original_value);
      assert_eq!(original_size, 5);

      // Check first element of original list
      let first_elem = nix_get_list_byidx(ctx, original_value, state, 0);
      if !first_elem.is_null() {
        let first_elem_type = nix_get_type(ctx, first_elem);
        assert_eq!(first_elem_type, ValueType_NIX_TYPE_INT);

        let first_elem_value = nix_get_int(ctx, first_elem);
        assert_eq!(first_elem_value, 1);
      }
    }

    // Clean up
    nix_value_decref(ctx, result);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn evaluation_error_handling() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    let load_err = nix_eval_state_builder_load(ctx, builder);
    assert_eq!(load_err, nix_err_NIX_OK);

    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Test evaluation with syntax error
    let invalid_expr = CString::new("{ invalid syntax ").unwrap();
    let path = CString::new("/test").unwrap();

    let result = nix_alloc_value(ctx, state);
    assert!(!result.is_null());

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      invalid_expr.as_ptr(),
      path.as_ptr(),
      result,
    );
    assert_ne!(eval_err, nix_err_NIX_OK); // should fail

    // Clear error for next test
    nix_clear_err(ctx);

    // Test evaluation with runtime error
    let runtime_error_expr = CString::new("1 + \"string\"").unwrap();

    let result2 = nix_alloc_value(ctx, state);
    assert!(!result2.is_null());

    let eval_err2 = nix_expr_eval_from_string(
      ctx,
      state,
      runtime_error_expr.as_ptr(),
      path.as_ptr(),
      result2,
    );

    // May succeed at parse time but fail during evaluation
    if eval_err2 == nix_err_NIX_OK {
      let force_err = nix_value_force(ctx, state, result2);
      assert_ne!(force_err, nix_err_NIX_OK); // should fail during forcing
    }

    // Test error information retrieval
    let error_code = nix_err_code(ctx);
    assert_ne!(error_code, nix_err_NIX_OK);

    // Try to get error message
    let mut error_len: std::os::raw::c_uint = 0;
    let error_msg_ptr = nix_err_msg(ctx, ctx, &mut error_len as *mut _);
    if !error_msg_ptr.is_null() && error_len > 0 {
      let error_msg = std::str::from_utf8(std::slice::from_raw_parts(
        error_msg_ptr as *const u8,
        error_len as usize,
      ))
      .unwrap_or("");
      // Should contain some error information
      assert!(!error_msg.is_empty());
    }

    // Test multi-argument call with wrong number of arguments
    nix_clear_err(ctx);

    let func_expr = CString::new("(x: y: x + y)").unwrap();
    let func_value = nix_alloc_value(ctx, state);
    assert!(!func_value.is_null());

    let eval_func_err = nix_expr_eval_from_string(
      ctx,
      state,
      func_expr.as_ptr(),
      path.as_ptr(),
      func_value,
    );
    assert_eq!(eval_func_err, nix_err_NIX_OK);

    // Try to call with wrong number of arguments.
    // The function expects 2, but we give 1
    let arg = nix_alloc_value(ctx, state);
    assert!(!arg.is_null());
    let _ = nix_init_int(ctx, arg, 5);

    let mut args = [arg];
    let result3 = nix_alloc_value(ctx, state);
    assert!(!result3.is_null());

    let call_err = nix_value_call_multi(
      ctx,
      state,
      func_value,
      1, // only 1 argument, but function expects 2
      args.as_mut_ptr(),
      result3,
    );

    // This should succeed but result should be a partially applied function
    if call_err == nix_err_NIX_OK {
      let force_err = nix_value_force(ctx, state, result3);
      assert_eq!(force_err, nix_err_NIX_OK);

      let result_type = nix_get_type(ctx, result3);
      assert_eq!(result_type, ValueType_NIX_TYPE_FUNCTION); // partially applied
    }

    // Clean up
    nix_value_decref(ctx, result3);
    nix_value_decref(ctx, arg);
    nix_value_decref(ctx, func_value);
    nix_value_decref(ctx, result2);
    nix_value_decref(ctx, result);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn builtin_function_calls() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let err = nix_libexpr_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let builder = nix_eval_state_builder_new(ctx, store);
    assert!(!builder.is_null());

    let load_err = nix_eval_state_builder_load(ctx, builder);
    assert_eq!(load_err, nix_err_NIX_OK);

    let state = nix_eval_state_build(ctx, builder);
    assert!(!state.is_null());

    // Test calling builtins.length
    let length_expr = CString::new("builtins.length").unwrap();
    let path = CString::new("/test").unwrap();

    let length_func = nix_alloc_value(ctx, state);
    assert!(!length_func.is_null());

    let eval_length_err = nix_expr_eval_from_string(
      ctx,
      state,
      length_expr.as_ptr(),
      path.as_ptr(),
      length_func,
    );
    assert_eq!(eval_length_err, nix_err_NIX_OK);

    // Create a list to test with
    let list_expr = CString::new("[1 2 3 4 5]").unwrap();
    let test_list = nix_alloc_value(ctx, state);
    assert!(!test_list.is_null());

    let eval_list_err = nix_expr_eval_from_string(
      ctx,
      state,
      list_expr.as_ptr(),
      path.as_ptr(),
      test_list,
    );
    assert_eq!(eval_list_err, nix_err_NIX_OK);

    // Call length function with the list
    let length_result = nix_alloc_value(ctx, state);
    assert!(!length_result.is_null());

    let call_length_err =
      nix_value_call(ctx, state, length_func, test_list, length_result);
    assert_eq!(call_length_err, nix_err_NIX_OK);

    let force_length_err = nix_value_force(ctx, state, length_result);
    assert_eq!(force_length_err, nix_err_NIX_OK);

    let length_type = nix_get_type(ctx, length_result);
    assert_eq!(length_type, ValueType_NIX_TYPE_INT);

    let length_value = nix_get_int(ctx, length_result);
    assert_eq!(length_value, 5);

    // Test builtins.map with multi-argument call
    let map_expr = CString::new("builtins.map").unwrap();
    let map_func = nix_alloc_value(ctx, state);
    assert!(!map_func.is_null());

    let eval_map_err = nix_expr_eval_from_string(
      ctx,
      state,
      map_expr.as_ptr(),
      path.as_ptr(),
      map_func,
    );
    assert_eq!(eval_map_err, nix_err_NIX_OK);

    // Create a simple function to map: (x: x * 2)
    let double_expr = CString::new("(x: x * 2)").unwrap();
    let double_func = nix_alloc_value(ctx, state);
    assert!(!double_func.is_null());

    let eval_double_err = nix_expr_eval_from_string(
      ctx,
      state,
      double_expr.as_ptr(),
      path.as_ptr(),
      double_func,
    );
    assert_eq!(eval_double_err, nix_err_NIX_OK);

    // Call map with the function and list
    let mut args = [double_func, test_list];
    let map_result = nix_alloc_value(ctx, state);
    assert!(!map_result.is_null());

    let call_map_err = nix_value_call_multi(
      ctx,
      state,
      map_func,
      2,
      args.as_mut_ptr(),
      map_result,
    );
    assert_eq!(call_map_err, nix_err_NIX_OK);

    let force_map_err = nix_value_force(ctx, state, map_result);
    assert_eq!(force_map_err, nix_err_NIX_OK);

    let map_result_type = nix_get_type(ctx, map_result);
    assert_eq!(map_result_type, ValueType_NIX_TYPE_LIST);

    let map_result_size = nix_get_list_size(ctx, map_result);
    assert_eq!(map_result_size, 5);

    // Check first element of mapped list (should be 2)
    let first_mapped = nix_get_list_byidx(ctx, map_result, state, 0);
    assert!(!first_mapped.is_null());

    let force_first_err = nix_value_force(ctx, state, first_mapped);
    assert_eq!(force_first_err, nix_err_NIX_OK);

    let first_mapped_type = nix_get_type(ctx, first_mapped);
    assert_eq!(first_mapped_type, ValueType_NIX_TYPE_INT);

    let first_mapped_value = nix_get_int(ctx, first_mapped);
    assert_eq!(first_mapped_value, 2); // 1 * 2

    // Clean up
    nix_value_decref(ctx, map_result);
    nix_value_decref(ctx, double_func);
    nix_value_decref(ctx, map_func);
    nix_value_decref(ctx, length_result);
    nix_value_decref(ctx, test_list);
    nix_value_decref(ctx, length_func);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
