<!--markdownlint-disable MD024-->

# nix-bindings

[![Rust Version](https://img.shields.io/badge/rust-1.85.0+-orange.svg)](https://www.rust-lang.org)

[C API]: https://nix.dev/manual/nix/2.32/c-api
[Nix]: https://nixos.org
[rust-bindgen]: https://github.com/rust-lang/rust-bindgen
[more accessible documentation]: https://notashelf.github.io/nix-bindings/nix_bindings_sys/index.html
[this blog post]: https://fzakaria.com/2025/08/17/using-nix-as-a-library
[discouraged by Bindgen]: https://rust-lang.github.io/rust-bindgen/cpp.html
[doxygen-bindgen]: https://github.com/rich-ayr/doxygen-bindgen

Rust FFI bindings and a robust high-level wrapper for the experimental [C API]
of the [Nix] build tool.

The goal of this repository is to provide generated bindings for the Nix C API,
then use those low-level bindings to build a safe, high-level Rust API for
interacting with Nix directly. In particular, `nix-bindings` aims to provide
programmatic access to Nix store operations from Rust without ever spawning
processes to interact with the Nix CLI.

The repository contains a large test suite, examples, safety documentation, and
[more accessible documentation] for the generated bindings. [^1]

[^1]: The Doxygen format used by Nix in the C API is not very easily parsable.
    The [doxygen-bindgen] crate does a pretty good job at this, but the
    documentation for the raw bindings, i.e. `nix-bindings-sys`, is not always
    up to standard. Either way, consider this a work in progress and something
    that will be improved over time as time and resources allow.

The **low-level** bindings generated via [rust-bindgen] are located in the
`nix-bindings-sys` crate in [`nix-bindings-sys/`](./nix-bindings-sys). By
default, the low-level bindings crate covers **all public Nix C API headers**,
including the store, evaluator, error, flake, and main APIs.

The [`nix-bindings/`](./nix-bindings) crate wraps `nix-bindings-sys` and
attempts to provide a more robust, **high-level** API for interacting with Nix
using Rust through a cleaner interface with additional safety documentation,
testing, examples and more.

> [!NOTE]
> Due to the limitations of rust-bindgen, there is no compatibility outside the
> public C API. As much as I would love to provide bindings for the C++ APIs,
> this seems very annoying and is [discouraged by Bindgen]. For interacting with
> C++ APIs, you might want to use C++ directly. See [this blog post] that
> inspired this repository for instructions on using Nix as a library in C++
> projects.

## Repository Layout

[![nix-bindings](https://img.shields.io/crates/v/nix-bindings)](https://crates.io/crates/nix-bindings)
[![nix-bindings-sys](https://img.shields.io/crates/v/nix-bindings-sys)](https://crates.io/crates/nix-bindings-sys)
[![Documentation](https://img.shields.io/docsrs/nix-bindings/latest)](https://docs.rs/nix-bindings/latest/)

For your convenience, this crate has been structured in a way that allows
providing low-level and high-level access to the C API at the same time through
different sub-crates. At the moment the crate layout is as follows:

```sh
.
├── nix-bindings-sys # raw, unsafe FFI bindings to the Nix C API
└── nix-bindings     # high-level, safe Rust API built on top of `nix-bindings-sys`
```

The `nix-bindings-sys` crate contains build wrapper (`build.rs`), as well as
tests and examples to demonstrate interacting with the generated bindings. Those
tests and examples are expected to pass, and they serve as a safeguard for when
API changes lead to breakage. This is one of the safety "promises" of this
repository, and act as safeguards against wildly breaking changes that affect
downstream. The `nix-bindings` crate contains the hand-written wrappers around
`nix-bindings-sys` and has additional helpers that you may be interested in.

[rendered documentation]: https://notashelf.github.io/nix-bindings/

Please see each crate's README for installation details. Additionally, the
[rendered documentation] contains documentation generated from Rust code
comments, examples and everything else provided by individual crate READMEs.

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

### Versioning

This crate uses a normalized semver scheme derived from the upstream Nix
version:

| Nix version     | Crate version |
| --------------- | ------------- |
| 2.32.7          | `0.2327.0`    |
| 2.32.7 + hotfix | `0.2327.1`    |

The crate version is `0.<major><minor><patch>.<crate_patch>`:

- The **minor** component encodes the full Nix version after the leading zero.
  For example, Nix `2.32.7` becomes `0.2327.0` (concatenation:
  `"2" + "32" + "7" = 2327`).
- The **patch** component is reserved for hotfixes to the Rust bindings that do
  not change the target Nix version. A fix to `0.2327.0` ships as `0.2327.1`.

Cargo's semver rules for `0.x.y` ensure that `^0.2327.0` will not pull in
`0.2328.0` (which targets a different Nix patch release) or `0.2330.0` (which
targets a different Nix minor release). Dependency consumers must update
explicitly when the target Nix version changes.

### Release branches

Each supported Nix release has a long-lived branch:

- `main` tracks the latest Nix version.
- `release/v0.2324` tracks hotfixes for bindings targeting Nix `2.32.4` (branch
  created from `v0.2324.0`).
- `release/v0.2327` tracks hotfixes for bindings targeting Nix `2.32.7` (branch
  created from `v0.2327.0`).

The version in `Cargo.toml` on a release branch is always the **next** pending
patch (e.g., `0.2324.1` after `v0.2324.0` was published). The `publish` workflow
tags the current version on merge, publishes it, then bumps the branch to the
next patch.

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
