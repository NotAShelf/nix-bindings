#![cfg(test)]

use serial_test::serial;
use std::ffi::CString;
use std::ptr;

use nix_bindings::*;

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

        // Parse a store path (I'm using a dummy path, will likely be invalid but should not segfault)
        // XXX: store_path may be null if path is invalid, but should not crash
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
        let s = unsafe { std::slice::from_raw_parts(start as *const u8, n as usize) };
        let s = std::str::from_utf8(s).unwrap();
        let out = user_data as *mut Option<String>;
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
            &mut uri as *mut _ as *mut _,
        );
        assert_eq!(res, nix_err_NIX_OK);
        assert!(uri.is_some());

        let mut storedir: Option<String> = None;
        let res = nix_store_get_storedir(
            ctx,
            store,
            Some(string_callback),
            &mut storedir as *mut _ as *mut _,
        );
        assert_eq!(res, nix_err_NIX_OK);
        assert!(storedir.is_some());

        nix_store_free(store);
        nix_c_context_free(ctx);
    }
}
