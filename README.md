# nix-bindings

[Nix C API]: https://nix.dev/manual/nix/2.28/c-api

Rust FFI bindings for the [Nix C API].

## Overview

[Nix]: https://nixos.or
[rust-bindgen]: https://github.com/rust-lang/rust-bindgen

**nix-bindings** provides safe(ish), idiomatic Rust bindings to the [Nix] build
tool's experimental C API. This enables direct, programmatic access to Nix store
operations, evaluation, and error handling from Rust, without shelling out to
the Nix CLI or depending on unstable C++ APIs.

Bindings are generated using [rust-bindgen] and cover all public Nix C API
headers, including store, evaluator, value, error, and flake support.

## Features

This crate is limited with the features of the C API, which is mostly documented
and hidden away for the time being. Regardless, some of features provided by
this crate (following the C API) are as follows:

- Open and interact with Nix stores (`nix_store_open`, `nix_store_realise`,
  etc.)
- Evaluate Nix expressions and call Nix functions (`nix_expr_eval_from_string`,
  `nix_value_call`, etc.)
- Manipulate store paths and values
- Access and set Nix configuration settings
- Retrieve and handle errors programmatically

I have also elected to add a test suite (work in progress) and some real-world
usage examples. While upstream does not promise stability, we are adding some
safety around what is available for (hopefully) nicer interactions with Nix as a
library.

---

## Usage

Add this crate to your `Cargo.toml` (local path or git, as appropriate):

```toml
[dependencies]
nix-bindings = { path = "./nix-bindings" }
```

You must have a compatible version of the Nix C API and development headers
installed. The build script uses `pkg-config` to locate the necessary libraries
and headers. You can take a look at the default Nix shell to get an idea of what
is available.

---

## Examples

See the [`examples/`](./examples) directory for small, self-contained usage
examples.

### Print Nix Library Version

```rust
use nix_bindings::nix_version_get;
use std::ffi::CStr;

fn main() {
    unsafe {
        let version_ptr = nix_version_get();
        if version_ptr.is_null() {
            eprintln!("Failed to get Nix version");
            std::process::exit(1);
        }
        let version = CStr::from_ptr(version_ptr).to_string_lossy();
        println!("Nix library version: {}", version);
    }
}
```

Run with:

```sh
cargo run --example version
```

## Testing

The tests suite is available in the tests directory, and can be ran easily with
`cargo test`.

Tests currently cover store operations, context management, configuration, and
more. All tests are integration-style and interact with a real Nix installation.

---

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

There are some caveats with this library. Namely, the C API is still unstable
and incomplete. Not everything is directly available, and we are severely
limited by what upstream provides to us. This also means that not all CLI
features are exposed. Some advanced or experimental features may require
additional or upstreaming work.

## License

See [LICENSE](./LICENSE) for details.
