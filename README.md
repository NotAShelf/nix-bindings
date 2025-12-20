# nix-bindings

[Nix C API]: https://nix.dev/manual/nix/2.32/c-api

Rust FFI bindings for the [Nix C API].

## Overview

[Nix]: https://nixos.or
[rust-bindgen]: https://github.com/rust-lang/rust-bindgen
[more accessible documentation]: https://notashelf.github.io/nix-bindings/nix_bindings/bindings/index.html
[this blog post]: https://fzakaria.com/2025/08/17/using-nix-as-a-library
[discouraged by Bindgen]: https://rust-lang.github.io/rust-bindgen/cpp.html

**nix-bindings** is a Rust crate that provides safe(ish), tested and idiomatic
Rust bindings for the [Nix] build tool's experimental C API with
[more accessible documentation]. [^1]

[^1]: Work in progress.

The goal of this repository is to ultimately use those low-level bindings that
provide direct, programmatic access to Nix store operations from Rust and use
them to construct a safe, high-level API to interact with Nix without ever
shelling out to the Nix CLI.

The **low-level** bindings located in `nix_bindings_sys` (or
`nix_bindings::sys`)are generated using [rust-bindgen] and cover all public Nix
C API headers, including store, evaluator, value, error, and flake support.

Due to the limitations of rust-bindgen, there is no compatibility outside the
public C API. As much as I would love to provide bindings for the C++ APIs, this
seems very annoying and is [discouraged by Bindgen]. For interacting with C++
APIs, you might want to use C++ directly. See [this blog post] that inspired
this repository for instructions on using Nix as a library in C++ projects.

### Repository Layout

> [!WARNING]
> The raw, unsafe FFI bindings are now located in the
> [`nix-bindings-sys`](./nix-bindings-sys/) crate directory. All low-level C API
> bindings and build logic are maintained there. This warning is placed in case
> you have previously accessed the low-level wrappers from the top-level. Most
> functions have been moved to `nix_bindings_sys` crate re-exported by
> nix-bindings.

This crate has been structured in a way that allows providing safe and unsafe
bindings at the same time, through different sub-crates. At the moment the crate
layout is as follows:

```plaintext
.
├── nix-bindings-sys # raw, unsafe FFI bindings to the Nix C API
└── nix-bindings # high-level, safe Rust API built on top of `nix-bindings-sys`
```

The `nix-bindings-sys` crate contains the build wrapper (`build.rs`), as well as
tests and examples to demonstrate interacting with the generated bindings. Those
tests and examples are expected to pass, and they serve as a safeguard for when
API changes lead to breakage.

### High-level Bindings: `nix-bindings-sys`

#### Features

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

using the generated Rust bindings.

> [!NOTE]
> I have elected to add a test suite [^2] and some real-world usage examples.
> While upstream does not promise stability, we are adding some safety around
> what is available for (hopefully) nicer interactions with Nix as a library.
> Those are manually written for available bindings based on the C headers.

[^2]: Also work in progress. Not all APIs are tested but I'd say around 95% of
    the bindings are properly tested.

#### Usage

Add nix-bindings to your `Cargo.toml`:

```toml
[dependencies]
nix-bindings = "2.28.4" # your Nix version must match the crate version.
```

You might also use a local path or Git as appropriate.

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

nix-bindings-sys is _meticulously_ tested for the sake of added safety and
soundness on top of the C API. The test suite is available in the
[`tests/`](./nix-bindings-sys/tests) directory and can be ran easily using
`cargo test`. For the time being the tests cover:

- Store operations
- Context management
- Configuration
- Expression evaluation
- Thunks

The tests are integration style, and interact with a real Nix installation. If
you believe a case is not appropriately tested, please create an issue. PRs are
also welcome to extend test cases :)

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
