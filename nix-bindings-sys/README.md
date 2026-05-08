# `nix-bindings-sys`

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

## Usage

Using `nix-binding-sys` is straightforward. Add it to your `Cargo.toml`:

```toml
[dependencies]
nix-bindings-sys = "0.2324.0" # your Nix version must match the crate version.
```

To pull only the C libraries you need, disable default features and opt in:

<!--markdownlint-disable MD013-->

```toml
[dependencies]
nix-bindings-sys = { version = "0.2324.0", default-features = false, features = ["store", "expr", "util"] }
```

<!--markdownlint-enable MD013-->

The default is to enable all five features for backwards compatibility and a
smoother experience `store`, `expr`, `util`, `flake`, `main`. `full` features
are available.

> [!TIP]
> This crate is tagged when a new Nix version is released and tested with. In
> the case your Nix version is incompatible with the published crate on
> <https://crates.io>, you might also use a local path or a Git remote in your
> `Cargo.toml` as you see fit. Published versions will be supported until
> upstream deprecates the version bindings were built for, after which the crate
> will be promptly yanked.

It is worth noting that you **must have a compatible version of the Nix C API
development headers**. The build script for `nix-bindings-sys` uses `pkg-config`
combined with feature flags to locate the necessary libraries and headers from
the environment, which is bootstrapped using Nix. You may look into `shell.nix`
to gen an idea of what is required to build nix-bindings. _Please do not create
issues for build errors unless you have verified that your environment setup is
correct_.

## Examples

There are various examples provided in
[`examples/`](./nix-bindings-sys/examples) directory to serve as small,
self-contained examples in case you decide to use this crate. You may run them
with `cargo run --example <example>`, e.g., `cargo run --example eval_basic`. In
addition to providing results, the code in the examples directory can help guide
you to use this library.

## Testing

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
