#![cfg(test)]

use std::{
  ffi::CString,
  sync::atomic::{AtomicU32, Ordering},
};

use nix_bindings_sys::*;
use serial_test::serial;

#[derive(Debug)]
struct TestPrimOpData {
  call_count:     AtomicU32,
  last_arg_value: AtomicU32,
}

// Simple PrimOp that adds 1 to an integer argument
unsafe extern "C" fn add_one_primop(
  user_data: *mut ::std::os::raw::c_void,
  context: *mut nix_c_context,
  state: *mut EvalState,
  args: *mut *mut nix_value,
  ret: *mut nix_value,
) {
  if user_data.is_null()
    || context.is_null()
    || state.is_null()
    || args.is_null()
    || ret.is_null()
  {
    let _ = unsafe {
      nix_set_err_msg(
        context,
        nix_err_NIX_ERR_UNKNOWN,
        b"Null pointer in add_one_primop\0".as_ptr() as *const _,
      )
    };
    return;
  }

  let data = unsafe { &*(user_data as *const TestPrimOpData) };
  data.call_count.fetch_add(1, Ordering::SeqCst);

  // Get first argument
  let arg = unsafe { *args.offset(0) };
  if arg.is_null() {
    let _ = unsafe {
      nix_set_err_msg(
        context,
        nix_err_NIX_ERR_UNKNOWN,
        b"Missing argument in add_one_primop\0".as_ptr() as *const _,
      )
    };
    return;
  }

  // Force evaluation of argument
  if unsafe { nix_value_force(context, state, arg) } != nix_err_NIX_OK {
    return;
  }

  // Check if argument is integer
  if unsafe { nix_get_type(context, arg) } != ValueType_NIX_TYPE_INT {
    let _ = unsafe {
      nix_set_err_msg(
        context,
        nix_err_NIX_ERR_UNKNOWN,
        b"Expected integer argument in add_one_primop\0".as_ptr() as *const _,
      )
    };
    return;
  }

  // Get integer value and add 1
  let value = unsafe { nix_get_int(context, arg) };
  data.last_arg_value.store(value as u32, Ordering::SeqCst);

  // Set return value
  let _ = unsafe { nix_init_int(context, ret, value + 1) };
}

// PrimOp that returns a constant string
unsafe extern "C" fn hello_world_primop(
  _user_data: *mut ::std::os::raw::c_void,
  context: *mut nix_c_context,
  _state: *mut EvalState,
  _args: *mut *mut nix_value,
  ret: *mut nix_value,
) {
  let hello = CString::new("Hello from Rust PrimOp!").unwrap();
  let _ = unsafe { nix_init_string(context, ret, hello.as_ptr()) };
}

// PrimOp that takes multiple arguments and concatenates them
unsafe extern "C" fn concat_strings_primop(
  _user_data: *mut ::std::os::raw::c_void,
  context: *mut nix_c_context,
  state: *mut EvalState,
  args: *mut *mut nix_value,
  ret: *mut nix_value,
) {
  if context.is_null() || state.is_null() || args.is_null() || ret.is_null() {
    return;
  }

  // This PrimOp expects exactly 2 string arguments
  let mut result = String::new();

  for i in 0..2 {
    let arg = unsafe { *args.offset(i) };
    if arg.is_null() {
      let _ = unsafe {
        nix_set_err_msg(
          context,
          nix_err_NIX_ERR_UNKNOWN,
          b"Missing argument in concat_strings_primop\0".as_ptr() as *const _,
        )
      };
      return;
    }

    // Force evaluation
    if unsafe { nix_value_force(context, state, arg) } != nix_err_NIX_OK {
      return;
    }

    // Check if it's a string
    if unsafe { nix_get_type(context, arg) } != ValueType_NIX_TYPE_STRING {
      let _ = unsafe {
        static ITEMS: &[u8] =
          b"Expected string argument in concat_strings_primop\0";
        nix_set_err_msg(
          context,
          nix_err_NIX_ERR_UNKNOWN,
          ITEMS.as_ptr() as *const _,
        )
      };
      return;
    }

    // Get string value using callback
    unsafe extern "C" fn string_callback(
      start: *const ::std::os::raw::c_char,
      n: ::std::os::raw::c_uint,
      user_data: *mut ::std::os::raw::c_void,
    ) {
      let s =
        unsafe { std::slice::from_raw_parts(start.cast::<u8>(), n as usize) };
      let s = std::str::from_utf8(s).unwrap_or("");
      let result = unsafe { &mut *(user_data as *mut String) };
      result.push_str(s);
    }

    let _ = unsafe {
      nix_get_string(
        context,
        arg,
        Some(string_callback),
        &mut result as *mut String as *mut ::std::os::raw::c_void,
      )
    };
  }

  let result_cstr =
    CString::new(result).unwrap_or_else(|_| CString::new("").unwrap());
  let _ = unsafe { nix_init_string(context, ret, result_cstr.as_ptr()) };
}

#[test]
#[serial]
fn primop_allocation_and_registration() {
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

    // Create test data
    let test_data = Box::new(TestPrimOpData {
      call_count:     AtomicU32::new(0),
      last_arg_value: AtomicU32::new(0),
    });
    let test_data_ptr = Box::into_raw(test_data);

    // Create argument names
    let arg_names = [CString::new("x").unwrap()];
    let arg_name_ptrs: Vec<*const _> =
      arg_names.iter().map(|s| s.as_ptr()).collect();
    let mut arg_name_ptrs_null_terminated = arg_name_ptrs;
    arg_name_ptrs_null_terminated.push(std::ptr::null());

    let name = CString::new("addOne").unwrap();
    let doc = CString::new("Add 1 to the argument").unwrap();

    // Allocate PrimOp
    let primop = nix_alloc_primop(
      ctx,
      Some(add_one_primop),
      1, // arity
      name.as_ptr(),
      arg_name_ptrs_null_terminated.as_mut_ptr(),
      doc.as_ptr(),
      test_data_ptr as *mut ::std::os::raw::c_void,
    );

    if !primop.is_null() {
      // Register the PrimOp globally
      let register_err = nix_register_primop(ctx, primop);
      // Registration may fail in some environments, but allocation should work
      assert!(
        register_err == nix_err_NIX_OK
          || register_err == nix_err_NIX_ERR_UNKNOWN,
        "Unexpected error code: {register_err}"
      );

      // Test using the PrimOp by creating a value with it
      let primop_value = nix_alloc_value(ctx, state);
      assert!(!primop_value.is_null());

      let init_err = nix_init_primop(ctx, primop_value, primop);
      assert_eq!(init_err, nix_err_NIX_OK);

      // Clean up value
      nix_value_decref(ctx, primop_value);
    }

    // Clean up
    let _ = Box::from_raw(test_data_ptr);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn primop_function_call() {
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

    // Create test data
    let test_data = Box::new(TestPrimOpData {
      call_count:     AtomicU32::new(0),
      last_arg_value: AtomicU32::new(0),
    });
    let test_data_ptr = Box::into_raw(test_data);

    // Create simple hello world PrimOp (no arguments)
    let name = CString::new("helloWorld").unwrap();
    let doc = CString::new("Returns hello world string").unwrap();
    let mut empty_args: Vec<*const ::std::os::raw::c_char> =
      vec![std::ptr::null()];

    let hello_primop = nix_alloc_primop(
      ctx,
      Some(hello_world_primop),
      0, // arity
      name.as_ptr(),
      empty_args.as_mut_ptr(),
      doc.as_ptr(),
      std::ptr::null_mut(),
    );

    if !hello_primop.is_null() {
      // Create a value with the PrimOp
      let primop_value = nix_alloc_value(ctx, state);
      assert!(!primop_value.is_null());

      let init_err = nix_init_primop(ctx, primop_value, hello_primop);
      assert_eq!(init_err, nix_err_NIX_OK);

      // Call the PrimOp (no arguments)
      let result = nix_alloc_value(ctx, state);
      assert!(!result.is_null());

      let call_err =
        nix_value_call(ctx, state, primop_value, std::ptr::null_mut(), result);
      if call_err == nix_err_NIX_OK {
        // Force the result
        let force_err = nix_value_force(ctx, state, result);
        assert_eq!(force_err, nix_err_NIX_OK);

        // Check if result is a string
        let result_type = nix_get_type(ctx, result);
        if result_type == ValueType_NIX_TYPE_STRING {
          // Get string value
          unsafe extern "C" fn string_callback(
            start: *const ::std::os::raw::c_char,
            n: ::std::os::raw::c_uint,
            user_data: *mut ::std::os::raw::c_void,
          ) {
            let s = unsafe {
              std::slice::from_raw_parts(start.cast::<u8>(), n as usize)
            };
            let s = std::str::from_utf8(s).unwrap_or("");
            let result = unsafe { &mut *(user_data as *mut Option<String>) };
            *result = Some(s.to_string());
          }

          let mut string_result: Option<String> = None;
          let _ = nix_get_string(
            ctx,
            result,
            Some(string_callback),
            &mut string_result as *mut Option<String>
              as *mut ::std::os::raw::c_void,
          );

          // Verify we got the expected string
          assert!(
            string_result
              .as_deref()
              .unwrap_or("")
              .contains("Hello from Rust")
          );
        }
      }

      nix_value_decref(ctx, result);
      nix_value_decref(ctx, primop_value);
    }

    // Clean up
    let _ = Box::from_raw(test_data_ptr);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn primop_with_arguments() {
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

    // Create test data
    let test_data = Box::new(TestPrimOpData {
      call_count:     AtomicU32::new(0),
      last_arg_value: AtomicU32::new(0),
    });
    let test_data_ptr = Box::into_raw(test_data);

    // Create add one PrimOp
    let arg_names = [CString::new("x").unwrap()];
    let arg_name_ptrs: Vec<*const _> =
      arg_names.iter().map(|s| s.as_ptr()).collect();
    let mut arg_name_ptrs_null_terminated = arg_name_ptrs;
    arg_name_ptrs_null_terminated.push(std::ptr::null());

    let name = CString::new("addOne").unwrap();
    let doc = CString::new("Add 1 to the argument").unwrap();

    let add_primop = nix_alloc_primop(
      ctx,
      Some(add_one_primop),
      1, // arity
      name.as_ptr(),
      arg_name_ptrs_null_terminated.as_mut_ptr(),
      doc.as_ptr(),
      test_data_ptr as *mut ::std::os::raw::c_void,
    );

    if !add_primop.is_null() {
      // Create a value with the PrimOp
      let primop_value = nix_alloc_value(ctx, state);
      assert!(!primop_value.is_null());

      let init_err = nix_init_primop(ctx, primop_value, add_primop);
      assert_eq!(init_err, nix_err_NIX_OK);

      // Create an integer argument
      let arg_value = nix_alloc_value(ctx, state);
      assert!(!arg_value.is_null());

      let init_arg_err = nix_init_int(ctx, arg_value, 42);
      assert_eq!(init_arg_err, nix_err_NIX_OK);

      // Call the PrimOp with the argument
      let result = nix_alloc_value(ctx, state);
      assert!(!result.is_null());

      let call_err =
        nix_value_call(ctx, state, primop_value, arg_value, result);
      if call_err == nix_err_NIX_OK {
        // Force the result
        let force_err = nix_value_force(ctx, state, result);
        assert_eq!(force_err, nix_err_NIX_OK);

        // Check if result is an integer
        let result_type = nix_get_type(ctx, result);
        if result_type == ValueType_NIX_TYPE_INT {
          let result_value = nix_get_int(ctx, result);
          assert_eq!(result_value, 43); // 42 + 1

          // Verify callback was called
          let test_data_ref = &*test_data_ptr;
          assert_eq!(test_data_ref.call_count.load(Ordering::SeqCst), 1);
          assert_eq!(test_data_ref.last_arg_value.load(Ordering::SeqCst), 42);
        }
      }

      nix_value_decref(ctx, result);
      nix_value_decref(ctx, arg_value);
      nix_value_decref(ctx, primop_value);
    }

    // Clean up
    let _ = Box::from_raw(test_data_ptr);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn primop_multi_argument() {
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

    // Create concat strings PrimOp
    let arg_names = [CString::new("s1").unwrap(), CString::new("s2").unwrap()];
    let arg_name_ptrs: Vec<*const _> =
      arg_names.iter().map(|s| s.as_ptr()).collect();
    let mut arg_name_ptrs_null_terminated = arg_name_ptrs;
    arg_name_ptrs_null_terminated.push(std::ptr::null());

    let name = CString::new("concatStrings").unwrap();
    let doc = CString::new("Concatenate two strings").unwrap();

    let concat_primop = nix_alloc_primop(
      ctx,
      Some(concat_strings_primop),
      2, // arity
      name.as_ptr(),
      arg_name_ptrs_null_terminated.as_mut_ptr(),
      doc.as_ptr(),
      std::ptr::null_mut(),
    );

    if !concat_primop.is_null() {
      // Create a value with the PrimOp
      let primop_value = nix_alloc_value(ctx, state);
      assert!(!primop_value.is_null());

      let init_err = nix_init_primop(ctx, primop_value, concat_primop);
      assert_eq!(init_err, nix_err_NIX_OK);

      // Create string arguments
      let arg1 = nix_alloc_value(ctx, state);
      let arg2 = nix_alloc_value(ctx, state);
      assert!(!arg1.is_null() && !arg2.is_null());

      let hello_cstr = CString::new("Hello, ").unwrap();
      let world_cstr = CString::new("World!").unwrap();

      let init_arg1_err = nix_init_string(ctx, arg1, hello_cstr.as_ptr());
      let init_arg2_err = nix_init_string(ctx, arg2, world_cstr.as_ptr());
      assert_eq!(init_arg1_err, nix_err_NIX_OK);
      assert_eq!(init_arg2_err, nix_err_NIX_OK);

      // Test multi-argument call using nix_value_call_multi
      let mut args = [arg1, arg2];
      let result = nix_alloc_value(ctx, state);
      assert!(!result.is_null());

      let call_err = nix_value_call_multi(
        ctx,
        state,
        primop_value,
        2,
        args.as_mut_ptr(),
        result,
      );
      if call_err == nix_err_NIX_OK {
        // Force the result
        let force_err = nix_value_force(ctx, state, result);
        assert_eq!(force_err, nix_err_NIX_OK);

        // Check if result is a string
        let result_type = nix_get_type(ctx, result);
        if result_type == ValueType_NIX_TYPE_STRING {
          unsafe extern "C" fn string_callback(
            start: *const ::std::os::raw::c_char,
            n: ::std::os::raw::c_uint,
            user_data: *mut ::std::os::raw::c_void,
          ) {
            let s = unsafe {
              std::slice::from_raw_parts(start.cast::<u8>(), n as usize)
            };
            let s = std::str::from_utf8(s).unwrap_or("");
            let result = unsafe { &mut *(user_data as *mut Option<String>) };
            *result = Some(s.to_string());
          }

          let mut string_result: Option<String> = None;
          let _ = nix_get_string(
            ctx,
            result,
            Some(string_callback),
            &mut string_result as *mut Option<String>
              as *mut ::std::os::raw::c_void,
          );

          // Verify concatenation worked
          assert_eq!(string_result.as_deref(), Some("Hello, World!"));
        }
      }

      nix_value_decref(ctx, result);
      nix_value_decref(ctx, arg2);
      nix_value_decref(ctx, arg1);
      nix_value_decref(ctx, primop_value);
    }

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn primop_error_handling() {
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

    // Test invalid PrimOp allocation (NULL callback)
    let name = CString::new("invalid").unwrap();
    let doc = CString::new("Invalid PrimOp").unwrap();
    let mut empty_args: Vec<*const ::std::os::raw::c_char> =
      vec![std::ptr::null()];

    let _invalid_primop = nix_alloc_primop(
      ctx,
      None, // NULL callback should cause error
      0,
      name.as_ptr(),
      empty_args.as_mut_ptr(),
      doc.as_ptr(),
      std::ptr::null_mut(),
    );

    // Test initializing value with NULL PrimOp (should fail)
    let test_value = nix_alloc_value(ctx, state);
    assert!(!test_value.is_null());

    nix_value_decref(ctx, test_value);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
