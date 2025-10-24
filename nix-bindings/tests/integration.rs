use std::sync::Arc;

use nix_bindings::{Context, EvalStateBuilder, Store, ValueType};
use serial_test::serial;

#[test]
#[serial]
fn test_basic_arithmetic() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test simple arithmetic
  let result = state
    .eval_from_string("5 + 3", "<test>")
    .expect("Failed to evaluate expression");

  assert_eq!(result.value_type(), ValueType::Int);
  assert_eq!(result.as_int().expect("Failed to get int value"), 8);
}

#[test]
#[serial]
fn test_string_operations() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test string literal
  let result = state
    .eval_from_string("\"hello world\"", "<test>")
    .expect("Failed to evaluate string");

  assert_eq!(result.value_type(), ValueType::String);
  assert_eq!(
    result.as_string().expect("Failed to get string"),
    "hello world"
  );
}

#[test]
#[serial]
fn test_boolean_values() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test boolean true
  let result = state
    .eval_from_string("true", "<test>")
    .expect("Failed to evaluate boolean");

  assert_eq!(result.value_type(), ValueType::Bool);
  assert!(result.as_bool().expect("Failed to get bool"));

  // Test boolean false
  let result = state
    .eval_from_string("false", "<test>")
    .expect("Failed to evaluate boolean");

  assert_eq!(result.value_type(), ValueType::Bool);
  assert!(!result.as_bool().expect("Failed to get bool"));
}

#[test]
#[serial]
fn test_complex_expressions() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test nested arithmetic
  let result = state
    .eval_from_string("(2 + 3) * (4 - 1)", "<test>")
    .expect("Failed to evaluate expression");

  assert_eq!(result.value_type(), ValueType::Int);
  assert_eq!(result.as_int().expect("Failed to get int value"), 15);

  // Test string interpolation
  let result = state
    .eval_from_string("\"The answer is ${toString (6 * 7)}\"", "<test>")
    .expect("Failed to evaluate string interpolation");

  assert_eq!(result.value_type(), ValueType::String);
  assert_eq!(
    result.as_string().expect("Failed to get string"),
    "The answer is 42"
  );
}

#[test]
#[serial]
fn test_attribute_sets() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test attribute set creation
  let result = state
    .eval_from_string("{ name = \"test\"; value = 42; }", "<test>")
    .expect("Failed to evaluate attrset");

  assert_eq!(result.value_type(), ValueType::Attrs);
}

#[test]
#[serial]
fn test_lists() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test list creation
  let result = state
    .eval_from_string("[ 1 2 3 \"hello\" ]", "<test>")
    .expect("Failed to evaluate list");

  assert_eq!(result.value_type(), ValueType::List);
}

#[test]
#[serial]
fn test_functions() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test function application
  let result = state
    .eval_from_string("(x: x + 1) 5", "<test>")
    .expect("Failed to evaluate function application");

  assert_eq!(result.value_type(), ValueType::Int);
  assert_eq!(result.as_int().expect("Failed to get int value"), 6);
}

#[test]
#[serial]
fn test_builtin_functions() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test length builtin
  let result = state
    .eval_from_string("builtins.length [ 1 2 3 4 5 ]", "<test>")
    .expect("Failed to evaluate builtin function");

  assert_eq!(result.value_type(), ValueType::Int);
  assert_eq!(result.as_int().expect("Failed to get int value"), 5);

  // Test head builtin
  let result = state
    .eval_from_string(
      "builtins.head [ \"first\" \"second\" \"third\" ]",
      "<test>",
    )
    .expect("Failed to evaluate builtin function");

  assert_eq!(result.value_type(), ValueType::String);
  assert_eq!(result.as_string().expect("Failed to get string"), "first");
}

#[test]
#[serial]
fn test_null_value() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test null value
  let result = state
    .eval_from_string("null", "<test>")
    .expect("Failed to evaluate null");

  assert_eq!(result.value_type(), ValueType::Null);
}

#[test]
#[serial]
fn test_error_handling() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test invalid expression - this should fail
  let result = state.eval_from_string("invalid syntax here", "<test>");
  assert!(result.is_err(), "Expected evaluation to fail");

  // Test type mismatch - trying to get int from string should fail
  let string_val = state
    .eval_from_string("\"not a number\"", "<test>")
    .expect("Failed to evaluate string");

  let int_result = string_val.as_int();
  assert!(int_result.is_err(), "Expected type conversion to fail");
}

#[test]
#[serial]
fn test_resource_cleanup() {
  // Test that resources are properly cleaned up when dropped
  for _i in 0..10 {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let _result = state
      .eval_from_string("1 + 1", "<test>")
      .expect("Failed to evaluate expression");

    // Resources should be automatically cleaned up when they go out of scope
  }
  // If we reach here without crashing, cleanup is working
}

#[test]
#[serial]
fn test_value_formatting_display() {
  let ctx = Arc::new(Context::new().expect("Failed to create context"));
  let store = Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
  let state = EvalStateBuilder::new(&store)
    .expect("Failed to create builder")
    .build()
    .expect("Failed to build state");

  // Test Display formatting
  let result = state
    .eval_from_string("42", "<test>")
    .expect("Failed to evaluate");
  println!("Display: {result}");
  assert_eq!(format!("{result}"), "42");

  let result = state
    .eval_from_string("\"hello world\"", "<test>")
    .expect("Failed to evaluate");
  println!("Display: {result}");
  assert_eq!(format!("{result}"), "hello world");

  // Test Debug formatting
  let result = state
    .eval_from_string("true", "<test>")
    .expect("Failed to evaluate");
  println!("Debug: {result:?}");
  assert_eq!(format!("{result:?}"), "Value::Bool(true)");

  // Test Nix syntax formatting
  let result = state
    .eval_from_string("\"test string\"", "<test>")
    .expect("Failed to evaluate");
  println!(
    "Nix syntax: {}",
    result.to_nix_string().expect("Failed to format")
  );
  assert_eq!(
    result.to_nix_string().expect("Failed to format"),
    "\"test string\""
  );

  // Test complex values
  let attrs = state
    .eval_from_string("{ a = 1; b = \"test\"; }", "<test>")
    .expect("Failed to evaluate");
  println!("Attrs display: {attrs}");
  println!("Attrs debug: {attrs:?}");

  let list = state
    .eval_from_string("[ 1 2 3 ]", "<test>")
    .expect("Failed to evaluate");
  println!("List display: {list}");
  println!("List debug: {list:?}");
}
