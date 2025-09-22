#![allow(non_upper_case_globals)]

use std::{
  ffi::{CStr, CString},
  ptr,
};

use nix_bindings_sys::{
  ValueType_NIX_TYPE_ATTRS,
  nix_c_context_create,
  nix_c_context_free,
  nix_err_NIX_OK,
  nix_eval_state_build,
  nix_eval_state_builder_free,
  nix_eval_state_builder_load,
  nix_eval_state_builder_new,
  nix_flake_settings_add_to_eval_state_builder,
  nix_flake_settings_free,
  nix_flake_settings_new,
  nix_get_attr_byname,
  nix_get_attrs_size,
  nix_get_bool,
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
};

/// Print a Nix attrset recursively (string keys, string/int values)
const MAX_RECURSION_DEPTH: usize = 16;

unsafe fn print_attrset(
  ctx: *mut nix_bindings_sys::nix_c_context,
  attrset: *mut nix_bindings_sys::nix_value,
  state: *mut nix_bindings_sys::EvalState,
  indent: usize,
  depth: usize,
) {
  if depth > MAX_RECURSION_DEPTH {
    println!(
      "{:indent$}[max recursion depth reached]",
      "",
      indent = indent
    );
    return;
  }
  let attr_count = unsafe { nix_get_attrs_size(ctx, attrset) };
  for i in 0..attr_count {
    let mut name_ptr: *const std::os::raw::c_char = ptr::null();
    let attr_val = unsafe {
      nix_bindings_sys::nix_get_attr_byidx(
        ctx,
        attrset,
        state,
        i,
        &mut name_ptr,
      )
    };
    if attr_val.is_null() || name_ptr.is_null() {
      println!("{:indent$}[invalid attr]", "", indent = indent);
      continue;
    }
    let name = unsafe { CStr::from_ptr(name_ptr) }.to_string_lossy();
    let typ = unsafe { nix_get_type(ctx, attr_val) };
    let type_name = unsafe { CStr::from_ptr(nix_get_typename(ctx, attr_val)) }
      .to_string_lossy();
    print!("{:indent$}{}: {} (", "", name, type_name, indent = indent);
    match typ {
      nix_bindings_sys::ValueType_NIX_TYPE_STRING => {
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
        let _ = unsafe {
          nix_get_string(
            ctx,
            attr_val,
            Some(string_cb),
            &mut got as *mut Option<String> as *mut std::ffi::c_void,
          )
        };
        println!("{:?})", got.as_deref().unwrap_or("[invalid utf8]"));
      },
      nix_bindings_sys::ValueType_NIX_TYPE_ATTRS => {
        println!();
        unsafe { print_attrset(ctx, attr_val, state, indent + 2, depth + 1) };
        println!("{:indent$})", "", indent = indent);
      },
      nix_bindings_sys::ValueType_NIX_TYPE_LIST => {
        let len = unsafe { nix_get_list_size(ctx, attr_val) };
        print!("[");
        for j in 0..len {
          let elem = unsafe { nix_get_list_byidx(ctx, attr_val, state, j) };
          if elem.is_null() {
            print!("<?>");
          } else {
            let elem_type = unsafe { nix_get_type(ctx, elem) };
            match elem_type {
              nix_bindings_sys::ValueType_NIX_TYPE_STRING => {
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
                let _ = unsafe {
                  nix_get_string(
                    ctx,
                    elem,
                    Some(string_cb),
                    &mut got as *mut Option<String> as *mut std::ffi::c_void,
                  )
                };
                print!("{:?}", got.as_deref().unwrap_or("[invalid utf8]"));
              },
              nix_bindings_sys::ValueType_NIX_TYPE_INT => {
                let v = unsafe { nix_get_int(ctx, elem) };
                print!("{}", v);
              },
              nix_bindings_sys::ValueType_NIX_TYPE_BOOL => {
                let v = unsafe { nix_get_bool(ctx, elem) };
                print!("{}", v);
              },
              nix_bindings_sys::ValueType_NIX_TYPE_ATTRS => {
                print!("\n");
                unsafe {
                  print_attrset(ctx, elem, state, indent + 4, depth + 1)
                };
              },
              nix_bindings_sys::ValueType_NIX_TYPE_NULL => {
                print!("null");
              },
              _ => print!("<?>"),
            }
          }
          if j + 1 < len {
            print!(", ");
          }
        }
        println!("])");
      },
      nix_bindings_sys::ValueType_NIX_TYPE_BOOL => {
        let v = unsafe { nix_get_bool(ctx, attr_val) };
        println!("{})", v);
      },
      nix_bindings_sys::ValueType_NIX_TYPE_INT => {
        let v = unsafe { nix_get_int(ctx, attr_val) };
        println!("{})", v);
      },
      nix_bindings_sys::ValueType_NIX_TYPE_NULL => {
        println!("null)");
      },
      _ => {
        println!("unhandled type)");
      },
    }
  }
}

fn main() {
  unsafe {
    // Create context and initialize libraries
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

    // Open the Nix store
    let store = nix_store_open(ctx, ptr::null(), ptr::null_mut());
    if store.is_null() {
      eprintln!("Failed to open Nix store");
      std::process::exit(1);
    }

    // Create an eval state builder
    let builder = nix_eval_state_builder_new(ctx, store);
    if builder.is_null() {
      eprintln!("Failed to create eval state builder");
      std::process::exit(1);
    }
    if nix_eval_state_builder_load(ctx, builder) != nix_err_NIX_OK {
      eprintln!("Failed to load eval state builder");
      std::process::exit(1);
    }

    // Create flake settings and add to builder
    let flake_settings = nix_flake_settings_new(ctx);
    if flake_settings.is_null() {
      eprintln!("Failed to create flake settings");
      std::process::exit(1);
    }
    let err = nix_flake_settings_add_to_eval_state_builder(
      ctx,
      flake_settings,
      builder,
    );
    if err != nix_err_NIX_OK {
      eprintln!(
        "Failed to add flake settings to eval state builder (err={})",
        err
      );
      nix_flake_settings_free(flake_settings);
      std::process::exit(1);
    }

    // Build the eval state
    let state = nix_eval_state_build(ctx, builder);
    if state.is_null() {
      eprintln!("Failed to build eval state");
      nix_flake_settings_free(flake_settings);
      std::process::exit(1);
    }

    // Evaluate a flake reference using `builtins.getFlake`
    // We'll evaluate this repository for the test case since it's simple enough
    // to be evaluated in a reasonable duration.
    let expr =
      CString::new("builtins.getFlake \"github:NotAShelf/nix-bindings\"")
        .unwrap();
    let path = CString::new("<flake>").unwrap();
    let mut value =
      std::mem::MaybeUninit::<nix_bindings_sys::nix_value>::uninit();

    let eval_err = nix_bindings_sys::nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      value.as_mut_ptr(),
    );
    if eval_err != nix_err_NIX_OK {
      eprintln!("Failed to evaluate flake reference (err={})", eval_err);
      nix_state_free(state);
      nix_flake_settings_free(flake_settings);
      nix_eval_state_builder_free(builder);
      nix_store_free(store);
      nix_c_context_free(ctx);
      std::process::exit(1);
    }

    let value_ptr = value.as_mut_ptr();
    let typ = nix_get_type(ctx, value_ptr);
    let type_name =
      CStr::from_ptr(nix_get_typename(ctx, value_ptr)).to_string_lossy();
    println!("Top-level value type: {} ({})", typ, type_name);

    // Print the top-level outputs attribute set of the flake
    if typ == ValueType_NIX_TYPE_ATTRS {
      println!("Flake outputs:");
      let outputs = nix_get_attr_byname(
        ctx,
        value_ptr,
        state,
        CString::new("outputs").unwrap().as_ptr(),
      );
      if !outputs.is_null()
        && nix_get_type(ctx, outputs) == ValueType_NIX_TYPE_ATTRS
      {
        print_attrset(ctx, outputs, state, 2, 0);
      } else {
        println!("  [no outputs attr or not an attrset]");
      }
    } else {
      println!("Result is not an attrset, cannot print outputs.");
    }

    nix_state_free(state);
    nix_flake_settings_free(flake_settings);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
