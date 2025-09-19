use std::{
  ffi::{CStr, CString},
  mem::MaybeUninit,
  ptr,
};

use nix_bindings::sys::{
  ValueType_NIX_TYPE_INT,
  nix_alloc_value,
  nix_c_context_create,
  nix_c_context_free,
  nix_err_NIX_OK,
  nix_eval_state_build,
  nix_eval_state_builder_free,
  nix_eval_state_builder_load,
  nix_eval_state_builder_new,
  nix_expr_eval_from_string,
  nix_get_int,
  nix_get_type,
  nix_get_typename,
  nix_init_int,
  nix_libexpr_init,
  nix_libstore_init,
  nix_libutil_init,
  nix_state_free,
  nix_store_free,
  nix_store_open,
  nix_value,
  nix_value_call,
  nix_value_force,
  nix_value_force_deep,
};

fn main() {
  unsafe {
    let ctx = nix_c_context_create();

    // Google called, they want their error "handling" back.
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

    // Evaluate a function: (x: x * 2)
    let expr = CString::new("(x: x * 2)").unwrap();
    let path = CString::new("<eval>").unwrap();
    let mut fn_val = MaybeUninit::<nix_value>::uninit();
    let eval_err = nix_expr_eval_from_string(
      ctx,
      state,
      expr.as_ptr(),
      path.as_ptr(),
      fn_val.as_mut_ptr(),
    );
    if eval_err != nix_err_NIX_OK {
      eprintln!("Failed to evaluate function expression");
      std::process::exit(1);
    }

    // Prepare argument: 21
    let arg_val = nix_alloc_value(ctx, state);
    nix_init_int(ctx, arg_val, 21);

    // Call the function
    let mut result_val = MaybeUninit::<nix_value>::uninit();
    let call_err = nix_value_call(
      ctx,
      state,
      fn_val.as_mut_ptr(),
      arg_val,
      result_val.as_mut_ptr(),
    );
    if call_err != nix_err_NIX_OK {
      eprintln!("Function application failed");
      std::process::exit(1);
    }

    // Force the result (should not be a thunk)
    let force_err = nix_value_force(ctx, state, result_val.as_mut_ptr());
    if force_err != nix_err_NIX_OK {
      eprintln!("Failed to force result value");
      std::process::exit(1);
    }

    // Inspect result
    let typ = nix_get_type(ctx, result_val.as_mut_ptr());
    let type_name =
      CStr::from_ptr(nix_get_typename(ctx, result_val.as_mut_ptr()))
        .to_string_lossy();
    println!("Result type: {typ} ({type_name})");

    if typ == ValueType_NIX_TYPE_INT {
      let v = nix_get_int(ctx, result_val.as_mut_ptr());
      println!("Function application result: {v}");
    } else {
      println!("Unexpected result type");
    }

    // Deep force (should be a no-op for int)
    let deep_force_err =
      nix_value_force_deep(ctx, state, result_val.as_mut_ptr());
    if deep_force_err != nix_err_NIX_OK {
      eprintln!("Failed to deep force result value");
      std::process::exit(1);
    }

    nix_state_free(state);
    nix_eval_state_builder_free(builder);
    nix_store_free(store);
    nix_c_context_free(ctx);
  }
}
