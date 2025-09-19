#![cfg(test)]

use std::ffi::{CStr, CString};

use nix_bindings_sys::*;
use serial_test::serial;

#[test]
#[serial]
fn context_create_and_free() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn libutil_init() {
  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);
    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn version_get() {
  unsafe {
    let version_ptr = nix_version_get();
    assert!(!version_ptr.is_null());
    let version = CStr::from_ptr(version_ptr).to_string_lossy();
    assert!(!version.is_empty(), "Version string should not be empty");
    assert!(
      version.chars().next().unwrap().is_ascii_digit(),
      "Version string should start with a digit: {version}"
    );
  }
}

#[test]
#[serial]
fn setting_set_and_get() {
  unsafe extern "C" fn string_callback(
    start: *const ::std::os::raw::c_char,
    n: ::std::os::raw::c_uint,
    user_data: *mut ::std::os::raw::c_void,
  ) {
    let s =
      unsafe { std::slice::from_raw_parts(start.cast::<u8>(), n as usize) };
    let s = std::str::from_utf8(s).unwrap();
    let out = user_data.cast::<Option<String>>();
    *unsafe { &mut *out } = Some(s.to_string());
  }

  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    // Set a setting (use a dummy/extra setting to avoid breaking global config)
    let key = CString::new("extra-test-setting").unwrap();
    let value = CString::new("test-value").unwrap();
    let set_err = nix_setting_set(ctx, key.as_ptr(), value.as_ptr());
    // Setting may not exist, but should not crash
    assert!(
      set_err == nix_err_NIX_OK || set_err == nix_err_NIX_ERR_KEY,
      "Unexpected error code: {set_err}"
    );

    // Try to get the setting (may not exist, but should not crash)
    let mut got: Option<String> = None;
    let get_err = nix_setting_get(
      ctx,
      key.as_ptr(),
      Some(string_callback),
      (&raw mut got).cast(),
    );
    assert!(
      get_err == nix_err_NIX_OK || get_err == nix_err_NIX_ERR_KEY,
      "Unexpected error code: {get_err}"
    );
    // If OK, we should have gotten a value
    if get_err == nix_err_NIX_OK {
      assert_eq!(got.as_deref(), Some("test-value"));
    }

    nix_c_context_free(ctx);
  }
}

#[test]
#[serial]
fn error_handling_apis() {
  unsafe extern "C" fn string_callback(
    start: *const ::std::os::raw::c_char,
    n: ::std::os::raw::c_uint,
    user_data: *mut ::std::os::raw::c_void,
  ) {
    let s =
      unsafe { std::slice::from_raw_parts(start.cast::<u8>(), n as usize) };
    let s = std::str::from_utf8(s).unwrap();
    let out = user_data.cast::<Option<String>>();
    *unsafe { &mut *out } = Some(s.to_string());
  }

  unsafe {
    let ctx = nix_c_context_create();
    assert!(!ctx.is_null());
    let err = nix_libutil_init(ctx);
    assert_eq!(err, nix_err_NIX_OK);

    // Set an error message
    let msg = CString::new("custom error message").unwrap();
    let set_err = nix_set_err_msg(ctx, nix_err_NIX_ERR_UNKNOWN, msg.as_ptr());
    assert_eq!(set_err, nix_err_NIX_ERR_UNKNOWN);

    // Get error code
    let code = nix_err_code(ctx);
    assert_eq!(code, nix_err_NIX_ERR_UNKNOWN);

    // Get error message
    let mut len: std::os::raw::c_uint = 0;
    let err_msg_ptr = nix_err_msg(ctx, ctx, &mut len as *mut _);
    if !err_msg_ptr.is_null() && len > 0 {
      let err_msg = std::str::from_utf8(std::slice::from_raw_parts(
        err_msg_ptr as *const u8,
        len as usize,
      ))
      .unwrap();
      assert!(err_msg.contains("custom error message"));
    }

    // Get error info message (should work, but may be empty)
    let mut info: Option<String> = None;
    let _ =
      nix_err_info_msg(ctx, ctx, Some(string_callback), (&raw mut info).cast());

    // Get error name (should work, but may be empty)
    let mut name: Option<String> = None;
    let _ =
      nix_err_name(ctx, ctx, Some(string_callback), (&raw mut name).cast());

    // Clear error
    nix_clear_err(ctx);
    let code_after_clear = nix_err_code(ctx);
    assert_eq!(code_after_clear, nix_err_NIX_OK);

    nix_c_context_free(ctx);
  }
}
