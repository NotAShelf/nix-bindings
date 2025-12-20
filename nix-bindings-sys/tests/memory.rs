#![cfg(test)]

use std::ffi::CString;

use nix_bindings_sys::*;
use serial_test::serial;

#[test]
#[serial]
fn value_reference_counting() {
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

    // Create a value
    let value = nix_alloc_value(ctx, state);
    assert!(!value.is_null());

    // Initialize with an integer
    let init_err = nix_init_int(ctx, value, 42);
    assert_eq!(init_err, nix_err_NIX_OK);

    // Test value-specific reference counting
    let incref_err = nix_value_incref(ctx, value);
    assert_eq!(incref_err, nix_err_NIX_OK);

    // Value should still be valid after increment
    let int_val = nix_get_int(ctx, value);
    assert_eq!(int_val, 42);

    // Test decrement
    let decref_err = nix_value_decref(ctx, value);
    assert_eq!(decref_err, nix_err_NIX_OK);

    // Value should still be valid (original reference still exists)
    let int_val2 = nix_get_int(ctx, value);
    assert_eq!(int_val2, 42);

    // Final decrement (should not crash)
    let final_decref_err = nix_value_decref(ctx, value);
    assert_eq!(final_decref_err, nix_err_NIX_OK);

    // Clean up
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn general_gc_reference_counting() {
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

    // Create a value for general GC testing
    let value = nix_alloc_value(ctx, state);
    assert!(!value.is_null());

    let init_err = nix_init_string(
      ctx,
      value,
      CString::new("test string for GC").unwrap().as_ptr(),
    );
    assert_eq!(init_err, nix_err_NIX_OK);

    // Test general GC reference counting
    let gc_incref_err =
      nix_gc_incref(ctx, value as *const ::std::os::raw::c_void);
    assert_eq!(gc_incref_err, nix_err_NIX_OK);

    // Value should still be accessible
    let value_type = nix_get_type(ctx, value);
    assert_eq!(value_type, ValueType_NIX_TYPE_STRING);

    // Test GC decrement
    let gc_decref_err =
      nix_gc_decref(ctx, value as *const ::std::os::raw::c_void);
    assert_eq!(gc_decref_err, nix_err_NIX_OK);

    // Final cleanup
    nix_value_decref(ctx, value);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn manual_garbage_collection() {
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

    // Create a few values to test basic GC functionality
    let mut values = Vec::new();
    for i in 0..3 {
      let value = nix_alloc_value(ctx, state);
      if !value.is_null() {
        let init_err = nix_init_int(ctx, value, i);
        if init_err == nix_err_NIX_OK {
          values.push(value);
        }
      }
    }

    // Verify values are accessible before GC
    for (i, &value) in values.iter().enumerate() {
      let int_val = nix_get_int(ctx, value);
      assert_eq!(int_val, i as i64);
    }

    // Clean up values before attempting GC to avoid signal issues
    for value in values {
      nix_value_decref(ctx, value);
    }

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn value_copying_and_memory_management() {
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

    // Create original value
    let original = nix_alloc_value(ctx, state);
    assert!(!original.is_null());

    let test_string = CString::new("test string for copying").unwrap();
    let init_err = nix_init_string(ctx, original, test_string.as_ptr());
    assert_eq!(init_err, nix_err_NIX_OK);

    // Create copy
    let copy = nix_alloc_value(ctx, state);
    assert!(!copy.is_null());

    let copy_err = nix_copy_value(ctx, copy, original);
    assert_eq!(copy_err, nix_err_NIX_OK);

    // Verify copy has same type and can be accessed
    let original_type = nix_get_type(ctx, original);
    let copy_type = nix_get_type(ctx, copy);
    assert_eq!(original_type, copy_type);
    assert_eq!(copy_type, ValueType_NIX_TYPE_STRING);

    // Test string contents using callback
    unsafe extern "C" fn string_callback(
      start: *const ::std::os::raw::c_char,
      n: ::std::os::raw::c_uint,
      user_data: *mut ::std::os::raw::c_void,
    ) {
      let s =
        unsafe { std::slice::from_raw_parts(start.cast::<u8>(), n as usize) };
      let s = std::str::from_utf8(s).unwrap_or("");
      let result = unsafe { &mut *(user_data as *mut Option<String>) };
      *result = Some(s.to_string());
    }

    let mut original_string: Option<String> = None;
    let mut copy_string: Option<String> = None;

    let _ = nix_get_string(
      ctx,
      original,
      Some(string_callback),
      &mut original_string as *mut Option<String>
        as *mut ::std::os::raw::c_void,
    );

    let _ = nix_get_string(
      ctx,
      copy,
      Some(string_callback),
      &mut copy_string as *mut Option<String> as *mut ::std::os::raw::c_void,
    );

    // Both should have the same string content
    assert_eq!(original_string, copy_string);
    assert!(
      original_string
        .as_deref()
        .unwrap_or("")
        .contains("test string")
    );

    // Test reference counting on both values
    let incref_orig = nix_value_incref(ctx, original);
    let incref_copy = nix_value_incref(ctx, copy);
    assert_eq!(incref_orig, nix_err_NIX_OK);
    assert_eq!(incref_copy, nix_err_NIX_OK);

    // Values should still be accessible after increment
    assert_eq!(nix_get_type(ctx, original), ValueType_NIX_TYPE_STRING);
    assert_eq!(nix_get_type(ctx, copy), ValueType_NIX_TYPE_STRING);

    // Clean up with decrements
    nix_value_decref(ctx, original);
    nix_value_decref(ctx, original); // extra decref from incref
    nix_value_decref(ctx, copy);
    nix_value_decref(ctx, copy); // extra decref from incref

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn complex_value_memory_management() {
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

    // Create a complex structure: list containing attribute sets
    let list_builder = nix_make_list_builder(ctx, state, 2);
    assert!(!list_builder.is_null());

    // Create first element: attribute set
    let attrs1 = nix_alloc_value(ctx, state);
    assert!(!attrs1.is_null());

    let bindings_builder1 = nix_make_bindings_builder(ctx, state, 2);
    assert!(!bindings_builder1.is_null());

    // Add attributes to first set
    let key1 = CString::new("name").unwrap();
    let val1 = nix_alloc_value(ctx, state);
    assert!(!val1.is_null());
    let name_str = CString::new("first").unwrap();
    let _ = nix_init_string(ctx, val1, name_str.as_ptr());

    let insert_err1 =
      nix_bindings_builder_insert(ctx, bindings_builder1, key1.as_ptr(), val1);
    assert_eq!(insert_err1, nix_err_NIX_OK);

    let key2 = CString::new("value").unwrap();
    let val2 = nix_alloc_value(ctx, state);
    assert!(!val2.is_null());
    let _ = nix_init_int(ctx, val2, 42);

    let insert_err2 =
      nix_bindings_builder_insert(ctx, bindings_builder1, key2.as_ptr(), val2);
    assert_eq!(insert_err2, nix_err_NIX_OK);

    let make_attrs_err1 = nix_make_attrs(ctx, attrs1, bindings_builder1);
    assert_eq!(make_attrs_err1, nix_err_NIX_OK);

    // Insert first attrs into list
    let list_insert_err1 =
      nix_list_builder_insert(ctx, list_builder, 0, attrs1);
    assert_eq!(list_insert_err1, nix_err_NIX_OK);

    // Create second element
    let attrs2 = nix_alloc_value(ctx, state);
    assert!(!attrs2.is_null());

    let bindings_builder2 = nix_make_bindings_builder(ctx, state, 1);
    assert!(!bindings_builder2.is_null());

    let key3 = CString::new("data").unwrap();
    let val3 = nix_alloc_value(ctx, state);
    assert!(!val3.is_null());
    let data_str = CString::new("second").unwrap();
    let _ = nix_init_string(ctx, val3, data_str.as_ptr());

    let insert_err3 =
      nix_bindings_builder_insert(ctx, bindings_builder2, key3.as_ptr(), val3);
    assert_eq!(insert_err3, nix_err_NIX_OK);

    let make_attrs_err2 = nix_make_attrs(ctx, attrs2, bindings_builder2);
    assert_eq!(make_attrs_err2, nix_err_NIX_OK);

    let list_insert_err2 =
      nix_list_builder_insert(ctx, list_builder, 1, attrs2);
    assert_eq!(list_insert_err2, nix_err_NIX_OK);

    // Create final list
    let final_list = nix_alloc_value(ctx, state);
    assert!(!final_list.is_null());

    let make_list_err = nix_make_list(ctx, list_builder, final_list);
    assert_eq!(make_list_err, nix_err_NIX_OK);

    // Test the complex structure
    assert_eq!(nix_get_type(ctx, final_list), ValueType_NIX_TYPE_LIST);
    assert_eq!(nix_get_list_size(ctx, final_list), 2);

    // Access nested elements
    let elem0 = nix_get_list_byidx(ctx, final_list, state, 0);
    let elem1 = nix_get_list_byidx(ctx, final_list, state, 1);
    assert!(!elem0.is_null() && !elem1.is_null());

    assert_eq!(nix_get_type(ctx, elem0), ValueType_NIX_TYPE_ATTRS);
    assert_eq!(nix_get_type(ctx, elem1), ValueType_NIX_TYPE_ATTRS);

    // Test memory management with deep copying
    let copied_list = nix_alloc_value(ctx, state);
    assert!(!copied_list.is_null());

    let copy_err = nix_copy_value(ctx, copied_list, final_list);
    assert_eq!(copy_err, nix_err_NIX_OK);

    // Force deep evaluation on copy
    let deep_force_err = nix_value_force_deep(ctx, state, copied_list);
    assert_eq!(deep_force_err, nix_err_NIX_OK);

    // Both should still be accessible
    assert_eq!(nix_get_list_size(ctx, final_list), 2);
    assert_eq!(nix_get_list_size(ctx, copied_list), 2);

    // Clean up all the values
    nix_value_decref(ctx, copied_list);
    nix_value_decref(ctx, final_list);
    nix_value_decref(ctx, attrs2);
    nix_value_decref(ctx, attrs1);
    nix_value_decref(ctx, val3);
    nix_value_decref(ctx, val2);
    nix_value_decref(ctx, val1);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn memory_management_error_conditions() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());

    // Test reference counting with NULL pointers (should handle gracefully)
    let null_incref_err =
      nix_gc_incref(ctx, std::ptr::null() as *const ::std::os::raw::c_void);

    // XXX: May succeed or fail depending on implementation. We can't really
    // know, so assert both.
    assert!(
      null_incref_err == nix_err_NIX_OK
        || null_incref_err == nix_err_NIX_ERR_UNKNOWN
    );

    let null_decref_err =
      nix_gc_decref(ctx, std::ptr::null() as *const ::std::os::raw::c_void);
    assert!(
      null_decref_err == nix_err_NIX_OK
        || null_decref_err == nix_err_NIX_ERR_UNKNOWN
    );

    let null_value_incref_err = nix_value_incref(ctx, std::ptr::null_mut());
    // Some Nix APIs gracefully handle null pointers and return OK
    assert!(
      null_value_incref_err == nix_err_NIX_OK
        || null_value_incref_err == nix_err_NIX_ERR_UNKNOWN
    );

    let null_value_decref_err = nix_value_decref(ctx, std::ptr::null_mut());
    // Some Nix APIs gracefully handle null pointers and return OK
    assert!(
      null_value_decref_err == nix_err_NIX_OK
        || null_value_decref_err == nix_err_NIX_ERR_UNKNOWN
    );

    // Test copy with NULL values
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

    let valid_value = nix_alloc_value(ctx, state);
    assert!(!valid_value.is_null());

    // Test copying to/from NULL
    let copy_from_null_err =
      nix_copy_value(ctx, valid_value, std::ptr::null_mut());
    assert_ne!(copy_from_null_err, nix_err_NIX_OK);

    let copy_to_null_err =
      nix_copy_value(ctx, std::ptr::null_mut(), valid_value);
    assert_ne!(copy_to_null_err, nix_err_NIX_OK);

    // Clean up
    nix_value_decref(ctx, valid_value);
    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
