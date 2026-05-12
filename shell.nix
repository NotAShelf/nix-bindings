{pkgs ? import <nixpkgs> {}}: let
  # XXX: update comment or pin specific version on `nix flake update`.
  # Generally the policy is to use the version corresponding to `pkgs.nix`'s
  # in nixos-stable. Fortunately the C API does not move fast enough, so
  # there is a degree of forward-compatibility.
  nixForBindings = pkgs.nixVersions.nix_2_34;
  inherit (pkgs.rustc) llvmPackages;
in
  pkgs.mkShell {
    name = "nix-bindings";

    strictDeps = true;
    nativeBuildInputs = with pkgs; [
      pkg-config
      cargo
      rustc
      llvmPackages.lld

      (rustfmt.override {asNightly = true;})
      rust-analyzer-unwrapped
      clippy
      taplo
      lldb

      # Additional Cargo tooling
      cargo-llvm-cov
      cargo-nextest
    ];

    buildInputs = [
      nixForBindings.dev
      pkgs.glibc.dev
    ];

    env = let
      inherit (llvmPackages) llvm;
    in {
      RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
      BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${pkgs.glibc.dev}";

      # `cargo-llvm-cov` reads these environment variables to find these binaries,
      # which are needed to run the tests
      LLVM_COV = "${llvm}/bin/llvm-cov";
      LLVM_PROFDATA = "${llvm}/bin/llvm-profdata";
    };
  }
