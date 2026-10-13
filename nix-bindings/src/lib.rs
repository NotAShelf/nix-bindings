#![warn(missing_docs)]
// The crate intentionally uses `Arc<Context>` for shared-ownership
// lifetime extension within a single thread. `Context` is neither `Send`
// nor `Sync`, which trips clippy's `arc_with_non_send_sync` lint on
// every `Arc::new(Context::new()?)` in the test suite even though the
// pattern is the documented one. See the crate-level `# Thread Safety`
// section.
#![cfg_attr(test, allow(clippy::arc_with_non_send_sync))]
//! High-level, safe Rust bindings for the Nix build tool.
//!
//! This crate provides ergonomic and idiomatic Rust APIs for interacting
//! with Nix using its C API.
//!
//! # Quick Start
//!
//! ```no_run
//! #[cfg(feature = "expr")]
//! {
//!   use std::sync::Arc;
//!
//!   use nix_bindings::{Context, EvalStateBuilder, Store};
//!
//!   fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let ctx = Arc::new(Context::new()?);
//!     let store = Arc::new(Store::open(&ctx, None)?);
//!     let state = EvalStateBuilder::new(&store)?.build()?;
//!
//!     let result = state.eval_from_string("1 + 2", "<eval>")?;
//!     println!("Result: {}", result.as_int()?);
//!
//!     Ok(())
//!   }
//! }
//! ```
//!
//! # Thread Safety
//!
//! Nix contexts and wrappers backed by them are neither `Send` nor `Sync`.
//! Construct, use, and drop them on the same thread. Per-call error buffers
//! are mutable, evaluator state is not safe for concurrent mutation, and
//! evaluator use requires Nix's garbage-collector thread registration.
//!
//! `Arc<Context>` extends ownership within that thread; it does not permit
//! sending a context or a dependent wrapper to another thread. A `Mutex`
//! does not remove this restriction. To do Nix work on a worker thread,
//! initialize and retain the wrappers there, and send only ordinary Rust
//! input and output data across threads. Do not assume independent contexts
//! isolate process-global settings or register additional evaluator threads.
//!
//! Primop callback arguments and return values borrow pointers valid only
//! for the current callback. They must not escape that call.
//!
//! # Value Formatting
//!
//! Values support multiple formatting options:
//!
//! ```no_run
//! #[cfg(feature = "expr")]
//! {
//!   use std::sync::Arc;
//!
//!   use nix_bindings::{Context, EvalStateBuilder, Store};
//!   fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let ctx = Arc::new(Context::new()?);
//!     let store = Arc::new(Store::open(&ctx, None)?);
//!     let state = EvalStateBuilder::new(&store)?.build()?;
//!     let value = state.eval_from_string("\"hello world\"", "<eval>")?;
//!
//!     // Display formatting (user-friendly)
//!     println!("{}", value); // => hello world
//!
//!     // Debug formatting (with type info)
//!     println!("{:?}", value); // => Value::String("hello world")
//!
//!     // Nix syntax formatting
//!     println!("{}", value.to_nix_string()?); // => "hello world"
//!     //
//!     Ok(())
//!   }
//! }
//! ```

/// Raw, unsafe FFI bindings to the Nix C API.
///
/// # Warning
///
/// This module exposes the low-level, unsafe C bindings. Prefer using the
/// safe, high-level APIs provided by this crate. Use at your own risk.
#[doc(hidden)]
pub mod sys {
  #[cfg(feature = "util")] pub use nix_bindings_sys::*;
}

mod error;
#[cfg(feature = "expr")] pub(crate) use error::check_ptr;
pub use error::{Error, Result};
// Shared FFI helpers used by the higher-level wrappers.
#[cfg(feature = "store")]
pub(crate) use error::{check_err, string_from_callback};

#[cfg(feature = "util")] mod context;
#[cfg(feature = "expr")] pub use context::is_pure_eval;
#[cfg(feature = "util")]
pub use context::{Context, Verbosity, nix_version};

#[cfg(feature = "store")] mod store;
#[cfg(feature = "store")]
pub use store::{CopyPathOptions, Derivation, Store, StorePath};

#[cfg(feature = "expr")] mod attrs;
#[cfg(feature = "expr")] mod eval;
#[cfg(feature = "expr")] mod lists;
#[cfg(feature = "expr")] mod value;
#[cfg(feature = "expr")] mod value_ops;

#[cfg(feature = "expr")] pub use attrs::AttrIterator;
#[cfg(feature = "expr")]
pub use eval::{EvalState, EvalStateBuilder};
#[cfg(feature = "expr")] pub use lists::ListIterator;
#[cfg(feature = "expr")] pub use value::{Value, ValueType};
#[cfg(feature = "expr")] pub use value_ops::NixValueOps;

#[cfg(feature = "external")] pub mod external;
#[cfg(feature = "flake")] pub mod flake;
#[cfg(feature = "primop")] pub mod primop;

#[cfg(all(test, any(feature = "store", feature = "expr")))]
mod tests {
  #[cfg(feature = "expr")] use std::sync::Arc;

  #[cfg(feature = "store")] use serial_test::serial;

  #[cfg(feature = "store")] use super::*;

  #[cfg(feature = "store")]
  #[test]
  #[serial]
  fn test_context_creation() {
    let _ctx = Context::new().expect("Failed to create context");
  }

  #[cfg(feature = "store")]
  #[test]
  #[serial]
  fn test_nix_version() {
    let version = nix_version();
    assert!(!version.is_empty(), "Version should not be empty");
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_eval_state_builder() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let _state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_simple_evaluation() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let result = state
      .eval_from_string("1 + 2", "<eval>")
      .expect("Failed to evaluate expression");

    assert_eq!(result.value_type(), ValueType::Int);
    assert_eq!(result.as_int().expect("Failed to get int value"), 3);
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_value_types() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let int_val = state
      .eval_from_string("42", "<eval>")
      .expect("Failed to evaluate int");
    assert_eq!(int_val.value_type(), ValueType::Int);
    assert_eq!(int_val.as_int().expect("Failed to get int"), 42);

    let bool_val = state
      .eval_from_string("true", "<eval>")
      .expect("Failed to evaluate bool");
    assert_eq!(bool_val.value_type(), ValueType::Bool);
    assert!(bool_val.as_bool().expect("Failed to get bool"));

    let str_val = state
      .eval_from_string("\"hello\"", "<eval>")
      .expect("Failed to evaluate string");
    assert_eq!(str_val.value_type(), ValueType::String);
    assert_eq!(str_val.as_string().expect("Failed to get string"), "hello");
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_value_construction() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let int_val = state.make_int(99).expect("Failed to make int");
    assert_eq!(int_val.as_int().unwrap(), 99);

    let float_val = state.make_float(2.5).expect("Failed to make float");
    assert!((float_val.as_float().unwrap() - 2.5).abs() < 1e-9);

    let bool_val = state.make_bool(true).expect("Failed to make bool");
    assert!(bool_val.as_bool().unwrap());

    let null_val = state.make_null().expect("Failed to make null");
    assert_eq!(null_val.value_type(), ValueType::Null);

    let str_val = state.make_string("hello").expect("Failed to make string");
    assert_eq!(str_val.as_string().unwrap(), "hello");
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_make_list() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let a = state.make_int(1).unwrap();
    let b = state.make_int(2).unwrap();
    let c = state.make_int(3).unwrap();

    let list = state.make_list(&[&a, &b, &c]).expect("Failed to make list");
    assert_eq!(list.value_type(), ValueType::List);
    assert_eq!(list.list_len().unwrap(), 3);
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_make_attrs() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let a = state.make_int(42).unwrap();
    let b = state.make_string("hello").unwrap();

    let attrs = state
      .make_attrs(&[("answer", &a), ("greeting", &b)])
      .expect("Failed to make attrs");
    assert_eq!(attrs.value_type(), ValueType::Attrs);

    let answer = attrs.get_attr("answer").unwrap();
    // as_int auto-forces lazy thunks.
    assert_eq!(answer.as_int().unwrap(), 42);
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_value_call() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let f = state
      .eval_from_string("x: x + 1", "<eval>")
      .expect("Failed to evaluate function");
    let arg = state.make_int(41).unwrap();
    let result = f.call(&arg).expect("Failed to call function");
    assert_eq!(result.as_int().unwrap(), 42);
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_value_copy() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let orig = state.make_int(7).unwrap();
    let copy = orig.copy().expect("Failed to copy value");
    assert_eq!(copy.as_int().unwrap(), 7);
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_as_string_with_context_plain() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let val = state
      .eval_from_string("\"hello\"", "<eval>")
      .expect("Failed to evaluate string");
    let (s, ctx_paths) = val
      .as_string_with_context()
      .expect("as_string_with_context failed");
    assert_eq!(s, "hello");
    assert!(
      ctx_paths.is_empty(),
      "Plain string should have no context paths"
    );
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_eval_from_file() {
    use std::io::Write as _;
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let mut tmp =
      tempfile::NamedTempFile::new().expect("Failed to create temp file");
    write!(tmp, "1 + 1").expect("Failed to write temp file");
    let result = state
      .eval_from_file(tmp.path())
      .expect("eval_from_file failed");
    assert_eq!(result.as_int().unwrap(), 2);
  }

  #[cfg(feature = "expr")]
  #[test]
  #[serial]
  fn test_no_load_config() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .no_load_config()
      .build()
      .expect("Failed to build state with no_load_config");
    let val = state
      .eval_from_string("1 + 1", "<eval>")
      .expect("Evaluation failed");
    assert_eq!(val.as_int().unwrap(), 2);
  }
}
