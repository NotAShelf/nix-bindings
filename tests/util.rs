#![cfg(test)]

use serial_test::serial;
use std::ffi::{CStr, CString};

use nix_bindings::*;

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
        let s = unsafe { std::slice::from_raw_parts(start.cast::<u8>(), n as usize) };
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
