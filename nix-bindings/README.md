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

A few key types are provided at the crate root and are gated by the `store` and
`expr` features (both included by default). Namely: `Context`, `EvalState`,
`EvalStateBuilder`, `Store`, `StorePath`, `Derivation`, `Value` and verbosity
may be used to manage C API context lifetime, expression evaluation, store
handle, derivation from JSON and Nix value with type-safe accessors
specifically. See [crate documentation] for more details.

## Usage

Add nix-bindings to your `Cargo.toml`:

```toml
# Cargo.toml
[dependencies]
nix-bindings = "0.2328.0"
```

The full crate (default) enables all modules. To exclude features you don't
need, disable defaults and pick:

<!--markdownlint-disable MD013-->

```toml
[dependencies]
nix-bindings = { version = "0.2324.0", default-features = false, features = ["store", "primop"] }
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
