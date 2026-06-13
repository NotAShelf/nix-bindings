# Contributing

<!--toc:start-->

- [Contributing](#contributing)
  - [Building and Testing](#building-and-testing)
  - [FFI Safety](#ffi-safety)
  - [Tests](#tests)
  - [AI-generated Content](#ai-generated-content)
  - [License](#license)

<!--toc:end-->

Thanks for considering a contribution to `nix-bindings`. This document is the
short list of rules; the bar for getting a PR merged.

## Building and Testing

`cargo build --workspace --all-features` and `cargo nextest run --workspace`
must both succeed before you open a PR. CI also enforces `cargo fmt` and
`cargo clippy --workspace --all-targets --all-features`.

The repository's `shell.nix` / `flake.nix` give you a shell with the right Nix C
headers; if you build outside it, you are responsible for matching the upstream
Nix version yourself.

## FFI Safety

For the sake of providing a sound, stable set of bindings the following are
disallowed unless appropriate reasoning is provided:

- Every `unsafe extern "C" fn` wraps its body in
  `panic::catch_unwind(AssertUnwindSafe(...))`. Panicking across the FFI
  boundary is UB.
- The public API surface is `Send`-only, never `Sync`. The `nix_c_context`
  error buffer is mutated on every call, so sharing `&T` across threads would
  race. New wrapper types follow the same rule.
- C++ shims use the upstream `nix_api_*_internal.h` headers and the
  `NIXC_CATCH_ERRS` macro. Do not redeclare upstream structs and do not let
  C++ exceptions escape.
- `nix_store_parse_path` throws a foreign C++ exception on non-store paths.
  Validate inputs first.

## Tests

PRs that add API surface add or extend a test for it. Look at neighbouring
tests for the shape; inline `#[cfg(test)] mod tests` for module-local
invariants, integration tests under `tests/` for flows that need a real
`EvalState` or `Store`.

## AI-generated Content

AI-_assisted_ content is allowed; you may, as long as it is appropriately
disclosed, send contributions where LLMs were used for coding assistance.You own
everything you submit and are responsible for verifying it. You **must**
disclose the model used, and the extent of its influence.

However, we will not allow the following under _any_ circumstances:

- AI-written commit messages, PR descriptions, or issue replies.
- AI-written code comments or docstrings
- AI-written `SAFETY:` comments. If you cannot state the invariants in your own
  words, the code is not ready.
- Drive-by AI refactors with no motivating issue.

If a significant chunk of a PR is AI-generated, say so in the description.
Disclosure is key. Any PR found to be in violation of these guidelines will be
closed without further discussion. Respect is the first principle of FOSS, and
the disallowed clauses ensure you don't violate this principle, intentionally or
otherwise.

## License

By contributing, you agree your changes are licensed under the repository's MIT
[LICENSE](./LICENSE).
