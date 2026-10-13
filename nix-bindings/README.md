# `nix-bindings`

This crate provides a safe, ergonomic Rust API built on top of
`nix-bindings-sys`. It wraps the raw FFI calls in idiomatic Rust types with
automatic resource management, type-safe conversions, and comprehensive error
handling. The crate is organized into the following modules, each gated by a
Cargo feature:

- **`store`** (`store` feature): Store, store path, and derivation management
  (opening stores, parsing store paths, realizing derivations, copying closures)
- **`attrs`** (requires `expr` feature): Attribute set access (`get_attr`,
  `has_attr`, `attr_keys`, `AttrIterator`)
- **`lists`** (requires `expr` feature): List operations (`list_len`,
  `list_get`, `list_iter`, `ListIterator`)
- **`flake`** (`flake`): Flake support (`FlakeSettings`, `FlakeReference`,
  `LockedFlake`, `LockFlags`, `FetchersSettings`)
- **`primop`** (`primop`): Custom Nix primitive operations via Rust closures
  (global builtins or value-embedded)
- **`external`** (`external`): Embed arbitrary Rust values as Nix external
  values with safe downcasting

[crate documentation]: https://notashelf.github.io/nix-bindings/nix_bindings/index.html

`Context`, version information, and verbosity are available with `util`.
The `main` feature adds plugin initialization and log-format configuration.
Store types require `store`; evaluation and value types require `expr`.
All are included by default. See [crate documentation] for their APIs.

## Usage

Add nix-bindings to your `Cargo.toml`:

```toml
# Cargo.toml
[dependencies]
nix-bindings = "0.2347.3"
```

The full crate (default) enables all modules. To exclude features you don't
need, disable defaults and pick:

<!--markdownlint-disable MD013-->

```toml
[dependencies]
nix-bindings = { version = "0.2347.3", default-features = false, features = ["store", "primop"] }
```

<!--markdownlint-enable MD013-->

Available features are `store`, `expr` (implies `store`), `flake`, `external`,
`primop`. `util` and `main` pass through to the underlying sys crate but do not
gate any high-level modules. `full` (default) enables everything.

Quick example evaluating a Nix expression:

```rust
use std::sync::Arc;
use nix_bindings::{Context, EvalStateBuilder, Store};

let ctx = Arc::new(Context::new()?);
let store = Arc::new(Store::open(&ctx, None)?);
let state = EvalStateBuilder::new(&store)?.build()?;

let result = state.eval_from_string("1 + 2", "<eval>")?;
println!("Result: {}", result.as_int()?);
```

Registering a custom primop as a global builtin (must happen before any
`EvalState` is built):

```rust
use std::sync::Arc;
use nix_bindings::{Context, EvalStateBuilder, Store, primop::PrimOp};

let ctx = Arc::new(Context::new()?);

PrimOp::new(&ctx, "double", 1, Some("Double an integer"), |args, ret| {
    let n = args[0].as_int()?;
    ret.set_int(n * 2)
})?
.register(&ctx)?;

let store = Arc::new(Store::open(&ctx, None)?);
let state = EvalStateBuilder::new(&store)?.build()?;
let result = state.eval_from_string("builtins.double 21", "<eval>")?;
assert_eq!(result.as_int()?, 42);
```

## Thread safety

Contexts and their dependent wrappers are neither `Send` nor `Sync`.
Construct, use, and drop them on the same thread. `Arc<Context>` shares
ownership within that thread; it does not make concurrent access safe.

Every C API call can mutate the context's error buffer, and evaluator threads
must satisfy Nix's garbage-collector registration requirements. A mutex does
not remove those requirements. See the `# Thread Safety` section in
`src/lib.rs` before integrating with worker threads.

## Testing

`nix-bindings` includes both unit tests (inline in each module) and integration
tests in [`tests/`](./nix-bindings/tests). Tests are gated by the same features
as the modules they exercise. Run them with:

```sh
# Full test suite
$ cargo nextest run -p nix-bindings

# Tests for a specific feature set
$ cargo nextest run -p nix-bindings --no-default-features --features store,expr

# You may use plain cargo test if you don't have cargo-nextest
$ cargo test -p nix-bindings
```

The test suite is rather large, and it covers a lot including:

- Store operations (open, query URI/dir/version, path validation)
- Expression evaluation (arithmetic, strings, booleans, functions,
  interpolation)
- Value construction and type conversion (int, float, bool, string, null)
- Attribute set manipulation (get, has, keys, iteration)
- List operations (length, indexing, iteration)
- String context handling (plain strings, store-path contexts)
- Derivation realization
- Resource cleanup (ensuring no leaks across repeated create/drop cycles)
- Flake settings and lock flags
- Custom primops (value-embedded)
- External values (creation, downcast, type-safe retrieval)
- Error handling (parse errors, type mismatches)
