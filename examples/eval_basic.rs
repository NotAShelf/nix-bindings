#![allow(non_upper_case_globals)]

use std::{
  ffi::{CStr, CString},
  mem::MaybeUninit,
  ptr,
};

use nix_bindings::{
  ValueType_NIX_TYPE_ATTRS,
  ValueType_NIX_TYPE_INT,
  ValueType_NIX_TYPE_LIST,
  ValueType_NIX_TYPE_STRING,
  nix_c_context_create,
  nix_c_context_free,
  nix_err_NIX_OK,
  nix_eval_state_build,
  nix_eval_state_builder_free,
  nix_eval_state_builder_load,
  nix_eval_state_builder_new,
  nix_expr_eval_from_string,
  nix_get_attr_byidx,
  nix_get_attrs_size,
  nix_get_int,
  nix_get_list_byidx,
  nix_get_list_size,
  nix_get_string,
  nix_get_type,
  nix_get_typename,
  nix_libexpr_init,
  nix_libstore_init,
  nix_libutil_init,
  nix_state_free,
  nix_store_free,
  nix_store_open,
  nix_value,
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

    // Example 1: Evaluate a simple expression and print its type and value
    let expr =
      CString::new("{ foo = 123; bar = [1 2 3]; baz = \"hello\"; }").unwrap();
    let path = CString::new("<eval>").unwrap();
    let mut value = MaybeUninit::<nix_value>::uninit();

    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      value.as_mut_ptr(),
    );
    if eval_err != nix_err_NIX_OK {
      eprintln!("Evaluation failed with error code: {eval_err}");
      nix_state_free(state);
      nix_eval_state_builder_free(builder);
      nix_store_free(store);
      nix_c_context_free(ctx);
      std::process::exit(1);
    }

    let value_ptr = value.as_mut_ptr();
    let typ = nix_get_type(ctx, value_ptr);
    let type_name =
      CStr::from_ptr(nix_get_typename(ctx, value_ptr)).to_string_lossy();
    println!("Top-level value type: {typ} ({type_name})");

    // Print attribute set contents
    if typ == ValueType_NIX_TYPE_ATTRS {
      let attr_count = nix_get_attrs_size(ctx, value_ptr);
      println!("Attrset has {attr_count} attributes:");
      for i in 0..attr_count {
        let mut name_ptr: *const std::os::raw::c_char = ptr::null();
        let attr_val =
          nix_get_attr_byidx(ctx, value_ptr, state, i, &mut name_ptr);
        if attr_val.is_null() || name_ptr.is_null() {
          println!("  [invalid attr]");
          continue;
        }
        let name = CStr::from_ptr(name_ptr).to_string_lossy();
        let attr_type = nix_get_type(ctx, attr_val);
        let attr_type_name =
          CStr::from_ptr(nix_get_typename(ctx, attr_val)).to_string_lossy();
        print!("  {name}: {attr_type_name} (");

        match attr_type {
          ValueType_NIX_TYPE_INT => {
            let v = nix_get_int(ctx, attr_val);
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
            let _ = nix_get_string(
              ctx,
              attr_val,
              Some(string_cb),
              (&raw mut got).cast(),
            );
            println!("{:?})", got.as_deref().unwrap_or("[invalid utf8]"));
          },
          ValueType_NIX_TYPE_LIST => {
            let len = nix_get_list_size(ctx, attr_val);
            print!("[");
            for j in 0..len {
              let elem = nix_get_list_byidx(ctx, attr_val, state, j);
              if elem.is_null() {
                print!("<?>");
              } else if nix_get_type(ctx, elem) == ValueType_NIX_TYPE_INT {
                print!("{}", nix_get_int(ctx, elem));
              } else {
                print!("<?>");
              }
              if j + 1 < len {
                print!(", ");
              }
            }
            println!("])");
          },
          _ => {
            println!("unhandled type)");
          },
        }
      }
    }

    // Example 2: Error handling for invalid expression
    let bad_expr = CString::new("this is not valid nix").unwrap();
    let mut bad_value = MaybeUninit::<nix_value>::uninit();
    let bad_err = nix_expr_eval_from_string(
      ctx,
      state,
      bad_expr.as_ptr(),
      path.as_ptr(),
      bad_value.as_mut_ptr(),
    );
    if bad_err == nix_err_NIX_OK {
      println!("Unexpectedly succeeded in evaluating invalid expression!");
    } else {
      println!(
        "Correctly failed to evaluate invalid expression (error code: \
         {bad_err})"
      );
    }

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
