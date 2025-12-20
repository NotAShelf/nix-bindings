{pkgs ? import <nixpkgs> {}}: let
  # FIXME: we eventually want to allow building for various Nix versions
  # in the same dev shell. Instead of a 'nixForBindings' variable *here*
  # it might be nicer to take environment variables which the build system
  # can respect to build them using the appropriate Nix version.
  nixForBindings = pkgs.nixVersions.nix_2_32;
in
  pkgs.mkShell {
    name = "nix-bindings";
    packages = with pkgs; [
      cargo
      rustc
      rustc.llvmPackages.lld

      (rustfmt.override {asNightly = true;})
      rust-analyzer-unwrapped
      clippy
      taplo
      lldb

      # Additional Cargo tooling
      cargo-llvm-cov
      cargo-nextest
    ];

    nativeBuildInputs = with pkgs; [
      nixForBindings.dev
      pkg-config
      glibc.dev
    ];

    buildInputs = [
      nixForBindings
    ];

    env = let
      inherit (pkgs.llvmPackages_19) llvm;
    in {
      RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${pkgs.glibc.dev}";

      # `cargo-llvm-cov` reads these environment variables to find these binaries,
      # which are needed to run the tests
      LLVM_COV = "${llvm}/bin/llvm-cov";
      LLVM_PROFDATA = "${llvm}/bin/llvm-profdata";
    };
  }
