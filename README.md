<!--markdownlint-disable MD024-->

# nix-bindings

[C API]: https://nix.dev/manual/nix/2.32/c-api
[Nix]: https://nixos.org
[rust-bindgen]: https://github.com/rust-lang/rust-bindgen
[more accessible documentation]: https://notashelf.github.io/nix-bindings/nix_bindings_sys/index.html
[this blog post]: https://fzakaria.com/2025/08/17/using-nix-as-a-library
[discouraged by Bindgen]: https://rust-lang.github.io/rust-bindgen/cpp.html
[doxygen-bindgen]: https://github.com/rich-ayr/doxygen-bindgen

Rust FFI bindings and high-level wrapper for the experimental [C API] of the
[Nix] build tool. The goal of this repository is to provide generated bindings
for the C API, ultimately use those low-level bindings that provide direct,
programmatic access to Nix store operations from Rust and use them to construct
a safe, high-level API to interact with Nix without ever shelling out to the Nix
CLI with the addition of a large test suite and [more accessible documentation].
[^1]

[^1]: The Doxygen format used by Nix in the C API is not very easily parsable.
    The [doxygen-bindgen] crate does a pretty good job at this, but the
    documentation for the raw bindings, i.e., `nix-bindings-sys` are not always
    up to standard. Either way, consider this a "work in progress" and something
    that will be improved over time as time and resources allow.

The **low-level** bindings generated via [rust-bindgen] are located in the
`nix-bindings-sys` crate located in [nix-bindings-sys/](./nix-bindings-sys). The
low-level bindings crate covers **all public Nix C API headers** with support
for store, evaulator, error and flake APIs.

The [`nix-bindings`](./nix-bindings) crate wraps `nix-bindings-sys` and attempts
to provide a more robust API for interacting with Nix using Rust through a
cleaner interface with additional safety documentation, testing, examples and
more. While all low-level logic is maintained in `nix-bindings-sys`, most

> [!NOTE]
> Due to the limitations of rust-bindgen, there is no compatibility outside the
> public C API. As much as I would love to provide bindings for the C++ APIs,
> this seems very annoying and is [discouraged by Bindgen]. For interacting with
> C++ APIs, you might want to use C++ directly. See [this blog post] that
> inspired this repository for instructions on using Nix as a library in C++
> projects.

## Repository Layout

For your convenience, this crate has been structured in a way that allows
providing low-level and high-level access to the C API at the same time through
different sub-crates. At the moment the crate layout is as follows:

```plaintext
.
├── nix-bindings-sys # raw, unsafe FFI bindings to the Nix C API
└── nix-bindings # high-level, safe Rust API built on top of `nix-bindings-sys`
```

The `nix-bindings-sys` crate contains build wrapper (`build.rs`), as well as
tests and examples to demonstrate interacting with the generated bindings. Those
tests and examples are expected to pass, and they serve as a safeguard for when
API changes lead to breakage. This is one of the safety "promises" of this
repository, and act as safeguards against wildly breaking changes that affect
downstream. The `nix-bindings` crate contains the hand-written wrappers around
`nix-bindings-sys` and has additional helpers that you may be interested in.

### `nix-bindings-sys`

This crate is limited with the features of the C API, which is mostly
undocumented and hidden away for the time being. Regardless, some of features
provided by this crate (following the C API) are as follows:

- Open and interact with Nix stores (`nix_store_open`, `nix_store_realise`,
  etc.)
- Evaluate Nix expressions and call Nix functions (`nix_expr_eval_from_string`,
  `nix_value_call`, etc.)

Additionally, with limited success nix-bindings allows you to:

- Manipulate store paths and values
- Access and set Nix configuration settings
- Retrieve and handle errors programmatically

All while using the generated Rust bindings.

#### Usage

Add nix-bindings to your `Cargo.toml`:

```toml
[dependencies]
nix-bindings = "2.32.4" # your Nix version must match the crate version.
```

You might also use a local path or a Git remote in your `Cargo.toml` as you see
fit. New versions will be published after testing, and will be supported until
upstream deprecates the version bindings were built for.

Do note that you _must_ have a compatible version of the Nix C API and
development headers available. The build script uses `pkg-config` to locate the
necessary libraries and headers from the environment, which is bootstrapped
using Nix. You may look into `shell.nix` to gen an idea of what is required to
build nix-bindings. Please do not create issues for build errors unless you have
verified that your environment setup is correct.

#### Examples

There are several examples provided [`examples/`](./nix-bindings-sys/examples)
directory to serve as small, self-contained examples in case you decide to use
this crate. You may run them with `cargo run --example <example>`, e.g.,
`cargo run --example eval_basic`. In addition to providing results, the code in
the examples directory can help guide you to use this library.

#### Testing

`nix-bindings-sys` is _meticulously_ tested for the sake of added safety and
soundness on top of the C API. The test suite is available in the
[`tests/`](./nix-bindings-sys/tests) directory and can be ran easily using
`cargo nextest run`. For the time being the tests cover:

- Store operations
- Context management
- Configuration
- Expression evaluation
- Thunks

The tests are integration style, and interact with a real Nix installation. If
you believe a case is not appropriately tested, please create an issue. PRs are
also welcome to extend test cases :)

### `nix-bindings`

This crate provides a safe, ergonomic Rust API built on top of
`nix-bindings-sys`. It wraps the raw FFI calls in idiomatic Rust types with
automatic resource management, type-safe conversions, and comprehensive error
handling. The crate is organized into the following public modules:

- **`store`**: Store, store path, and derivation management (opening stores,
  parsing store paths, realizing derivations, copying closures)
- **`attrs`**: Attribute set access (`get_attr`, `has_attr`, `attr_keys`,
  `AttrIterator`)
- **`lists`**: List operations (`list_len`, `list_get`, `list_iter`,
  `ListIterator`)
- **`flake`**: Flake support (`FlakeSettings`, `FlakeReference`, `LockedFlake`,
  `LockFlags`, `FetchersSettings`)
- **`primop`**: Custom Nix primitive operations via Rust closures (global
  builtins or value-embedded)
- **`external`**: Embed arbitrary Rust values as Nix external values with safe
  downcasting

[crate documentation]: https://notashelf.github.io/nix-bindings/nix_bindings/index.html

A few key types are provided at the crate root. Namely: `Context`, `EvalState`,
`EvalStateBuilder`, `Store`, `StorePath`, `Derivation`, `Value` and verbosity
may be used to manage C API context lifetime, expression evaluation, store
handle, derivation from JSON and Nix value with type-safe accessors
specifically. See [crate documentation] for more details.

#### Usage

Add nix-bindings to your `Cargo.toml`:

```toml
[dependencies]
nix-bindings = "2.32.4"
```

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

#### Testing

`nix-bindings` includes both unit tests (inline in each module) and integration
tests in [`tests/`](./nix-bindings/tests). Run them with:

```sh
# cargo-nextest is provided by the devshell, and provides faster test invocation
$ cargo nextest run -p nix-bindings

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

## Contributing

Contributions are welcome! If you have noticed something missing and would like
to patch it yourself, I would appreciate contributions. Please:

- Keep examples and tests small, focused, and idiomatic
- Follow Rust FFI safety best practices
- Add (or update) tests for any new API surface
- Document any new or changed behavior

If you encounter issues with the bindings or Nix C API compatibility, please
open an issue or submit a PR with a minimal reproduction. Note that some issues
are to be filed _upstream_, so make sure you try the C API _directly_ before
filing an issue with the bindings. I am not going to close any issues for
missing C testing, but it will make our lives easier in identifying the issue.

## Caveats

[relevant section in the Nix manual]: https://nix.dev/manual/nix/2.32/c-api

There are some caveats with this library. Namely, the C API is still unstable
and incomplete. Not everything is directly available, and we are severely
limited by what upstream provides to us. Upstream calls this API _"C API with
the intent of becoming a stable API, which it is currently not."_ See the
[relevant section in the Nix manual] for more details and appropriate
communication channels.

This also means that not all CLI features are exposed. Some advanced or
experimental features may require additional or upstreaming work.

## License

See [LICENSE](./LICENSE) for details.
