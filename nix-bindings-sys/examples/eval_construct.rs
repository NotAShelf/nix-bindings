use std::{
  ffi::{CStr, CString},
  ptr,
};

use nix_bindings::sys::{
  ValueType_NIX_TYPE_INT,
  ValueType_NIX_TYPE_STRING,
  nix_alloc_value,
  nix_bindings_builder_free,
  nix_bindings_builder_insert,
  nix_c_context_create,
  nix_c_context_free,
  nix_err_NIX_OK,
  nix_eval_state_build,
  nix_eval_state_builder_free,
  nix_eval_state_builder_load,
  nix_eval_state_builder_new,
  nix_get_attr_byidx,
  nix_get_attrs_size,
  nix_get_int,
  nix_get_list_byidx,
  nix_get_list_size,
  nix_get_string,
  nix_get_type,
  nix_get_typename,
  nix_init_int,
  nix_init_string,
  nix_libexpr_init,
  nix_libstore_init,
  nix_libutil_init,
  nix_list_builder_free,
  nix_list_builder_insert,
  nix_make_attrs,
  nix_make_bindings_builder,
  nix_make_list,
  nix_make_list_builder,
  nix_state_free,
  nix_store_free,
  nix_store_open,
};

fn main() {
  unsafe {
    let ctx = nix_c_context_create();
    if ctx.is_null() {
      eprintln!("Failed to create Nix context");
      std::process::exit(1);
    }
    if nix_libutil_init(ctx) != nix_err_NIX_OK {
      eprintln!("Failed to init libutil");
      std::process::exit(1);
    }
    if nix_libstore_init(ctx) != nix_err_NIX_OK {
      eprintln!("Failed to init libstore");
      std::process::exit(1);
    }
    if nix_libexpr_init(ctx) != nix_err_NIX_OK {
      eprintln!("Failed to init libexpr");
      std::process::exit(1);
    }

    let store = nix_store_open(ctx, ptr::null(), ptr::null_mut());
    if store.is_null() {
      eprintln!("Failed to open Nix store");
      std::process::exit(1);
    }

    let builder = nix_eval_state_builder_new(ctx, store);
    if builder.is_null() {
      eprintln!("Failed to create eval state builder");
      std::process::exit(1);
    }
    if nix_eval_state_builder_load(ctx, builder) != nix_err_NIX_OK {
      eprintln!("Failed to load eval state builder");
      std::process::exit(1);
    }
    let state = nix_eval_state_build(ctx, builder);
    if state.is_null() {
      eprintln!("Failed to build eval state");
      std::process::exit(1);
    }

    // Build a Nix list: [10, 20, 30]
    let list_builder = nix_make_list_builder(ctx, state, 3);
    if list_builder.is_null() {
      eprintln!("Failed to create list builder");
      std::process::exit(1);
    }
    let v1 = nix_alloc_value(ctx, state);
    let v2 = nix_alloc_value(ctx, state);
    let v3 = nix_alloc_value(ctx, state);
    nix_init_int(ctx, v1, 10);
    nix_init_int(ctx, v2, 20);
    nix_init_int(ctx, v3, 30);
    nix_list_builder_insert(ctx, list_builder, 0, v1);
    nix_list_builder_insert(ctx, list_builder, 1, v2);
    nix_list_builder_insert(ctx, list_builder, 2, v3);

    let list_val = nix_alloc_value(ctx, state);
    if nix_make_list(ctx, list_builder, list_val) != nix_err_NIX_OK {
      eprintln!("Failed to make list");
      std::process::exit(1);
    }
    nix_list_builder_free(list_builder);

    println!("Constructed Nix list:");
    let len = nix_get_list_size(ctx, list_val);
    print!("[");
    for i in 0..len {
      let elem = nix_get_list_byidx(ctx, list_val, state, i);
      if elem.is_null() {
        print!("<?>");
      } else if nix_get_type(ctx, elem) == ValueType_NIX_TYPE_INT {
        print!("{}", nix_get_int(ctx, elem));
      } else {
        print!("<?>");
      }
      if i + 1 < len {
        print!(", ");
      }
    }
    println!("]");

    // Build a Nix attrset: { a = 1; b = "foo"; }
    let attr_builder = nix_make_bindings_builder(ctx, state, 2);
    if attr_builder.is_null() {
      eprintln!("Failed to create bindings builder");
      std::process::exit(1);
    }
    let a_val = nix_alloc_value(ctx, state);
    let b_val = nix_alloc_value(ctx, state);
    nix_init_int(ctx, a_val, 1);
    let foo = CString::new("foo").unwrap();
    nix_init_string(ctx, b_val, foo.as_ptr());
    let a = CString::new("a").unwrap();
    let b = CString::new("b").unwrap();
    nix_bindings_builder_insert(ctx, attr_builder, a.as_ptr(), a_val);
    nix_bindings_builder_insert(ctx, attr_builder, b.as_ptr(), b_val);

    let attr_val = nix_alloc_value(ctx, state);
    if nix_make_attrs(ctx, attr_val, attr_builder) != nix_err_NIX_OK {
      eprintln!("Failed to make attrset");
      std::process::exit(1);
    }
    nix_bindings_builder_free(attr_builder);

    println!("Constructed Nix attrset:");
    let attr_count = nix_get_attrs_size(ctx, attr_val);
    for i in 0..attr_count {
      let mut name_ptr: *const std::os::raw::c_char = ptr::null();
      let attr = nix_get_attr_byidx(ctx, attr_val, state, i, &mut name_ptr);
      if attr.is_null() || name_ptr.is_null() {
        println!("  [invalid attr]");
        continue;
      }
      let name = CStr::from_ptr(name_ptr).to_string_lossy();
      let typ = nix_get_type(ctx, attr);
      let type_name =
        CStr::from_ptr(nix_get_typename(ctx, attr)).to_string_lossy();
      print!("  {name}: {type_name} (");
      match typ {
        ValueType_NIX_TYPE_INT => {
          let v = nix_get_int(ctx, attr);
          println!("{v})");
        },
        ValueType_NIX_TYPE_STRING => {
          extern "C" fn string_cb(
            start: *const ::std::os::raw::c_char,
            n: ::std::os::raw::c_uint,
            user_data: *mut ::std::os::raw::c_void,
          ) {
            let s = unsafe {
              std::slice::from_raw_parts(start.cast::<u8>(), n as usize)
            };
            let s = std::str::from_utf8(s).unwrap();
            let out = user_data.cast::<Option<String>>();
            unsafe { *out = Some(s.to_string()) };
          }
          let mut got: Option<String> = None;
          let _ =
            nix_get_string(ctx, attr, Some(string_cb), (&raw mut got).cast());
          println!("{:?})", got.as_deref().unwrap_or("[invalid utf8]"));
        },
        _ => {
          println!("unhandled type)");
        },
      }
    }

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
