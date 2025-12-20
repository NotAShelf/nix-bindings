#![cfg(test)]

use std::{ffi::CString, ptr};

use nix_bindings_sys::*;
use serial_test::serial;

#[test]
#[serial]
fn libstore_init_and_open_free() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    // Open the default store (NULL URI, NULL params)
    let store = nix_store_open(ctx, ptr::null(), ptr::null_mut());
    assert!(!store.is_null());

    // Free the store and context
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn parse_and_clone_free_store_path() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, ptr::null(), ptr::null_mut());
    assert!(!store.is_null());

    // Parse a store path (I'm using a dummy path, will likely be invalid but
    // should not segfault) XXX: store_path may be null if path is invalid,
    // but should not crash
    let path_str = CString::new("/nix/store/dummy-path").unwrap();
    let store_path = nix_store_parse_path(ctx, store, path_str.as_ptr());

    if !store_path.is_null() {
      // Clone and free
      let cloned = nix_store_path_clone(store_path);
      assert!(!cloned.is_null());
      nix_store_path_free(cloned);
      nix_store_path_free(store_path);
    }

    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn store_get_uri_and_storedir() {
  unsafe extern "C" fn string_callback(
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

  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, ptr::null(), ptr::null_mut());
    assert!(!store.is_null());

    let mut uri: Option<String> = None;
    let res = nix_store_get_uri(
      ctx,
      store,
      Some(string_callback),
      (&raw mut uri).cast(),
    );
    assert_eq!(res, nix_err_NIX_OK);
    assert!(uri.is_some());

    let mut storedir: Option<String> = None;
    let res = nix_store_get_storedir(
      ctx,
      store,
      Some(string_callback),
      (&raw mut storedir).cast(),
    );
    assert_eq!(res, nix_err_NIX_OK);
    assert!(storedir.is_some());

    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn libstore_init_no_load_config() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init_no_load_config(ctx);
    assert_eq!(err, nix_err_NIX_OK);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn store_is_valid_path_and_real_path() {
  unsafe extern "C" fn string_callback(
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

  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    // Use a dummy path (should not be valid, but should not crash)
    let path_str = CString::new("/nix/store/dummy-path").unwrap();
    let store_path = nix_store_parse_path(ctx, store, path_str.as_ptr());
    if !store_path.is_null() {
      let valid = nix_store_is_valid_path(ctx, store, store_path);
      assert!(!valid, "Dummy path should not be valid");

      let mut real_path: Option<String> = None;
      let res = nix_store_real_path(
        ctx,
        store,
        store_path,
        Some(string_callback),
        (&raw mut real_path).cast(),
      );
      // May fail, but should not crash
      assert!(res == nix_err_NIX_OK || res == nix_err_NIX_ERR_UNKNOWN);
      nix_store_path_free(store_path);
    }

    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn store_path_name() {
  unsafe extern "C" fn string_callback(
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

  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let path_str = CString::new("/nix/store/foo-bar-baz").unwrap();
    let store_path = nix_store_parse_path(ctx, store, path_str.as_ptr());
    if !store_path.is_null() {
      let mut name: Option<String> = None;
      nix_store_path_name(
        store_path,
        Some(string_callback),
        (&raw mut name).cast(),
      );
      // Should extract the name part ("foo-bar-baz")
      assert!(name.as_deref().unwrap_or("").contains("foo-bar-baz"));
      nix_store_path_free(store_path);
    }

    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn store_get_version() {
  unsafe extern "C" fn string_callback(
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

  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    let mut version: Option<String> = None;
    let res = nix_store_get_version(
      ctx,
      store,
      Some(string_callback),
      (&raw mut version).cast(),
    );
    assert_eq!(res, nix_err_NIX_OK);
    // Version may be empty for dummy stores, but should not crash
    assert!(version.is_some());

    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn store_realise_and_copy_closure() {
  unsafe extern "C" fn realise_callback(
    _userdata: *mut ::std::os::raw::c_void,
    outname: *const ::std::os::raw::c_char,
    out: *const StorePath,
  ) {
    // Just check that callback is called with non-null pointers
    assert!(!outname.is_null());
    assert!(!out.is_null());
  }

  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libstore_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    let store = nix_store_open(ctx, std::ptr::null(), std::ptr::null_mut());
    assert!(!store.is_null());

    // Use a dummy path (should not crash, may not realise)
    let path_str = CString::new("/nix/store/dummy-path").unwrap();
    let store_path = nix_store_parse_path(ctx, store, path_str.as_ptr());
    if !store_path.is_null() {
      // Realise (should fail, but must not crash)
      let _ = nix_store_realise(
        ctx,
        store,
        store_path,
        std::ptr::null_mut(),
        Some(realise_callback),
      );

      // Copy closure to same store (should fail, but must not crash)
      let _ = nix_store_copy_closure(ctx, store, store, store_path);

      nix_store_path_free(store_path);
    }

    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
