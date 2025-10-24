{pkgs ? import <nixpkgs> {}}: let
  nixForBindings = pkgs.nixVersions.nix_2_28;
  lixForBindings = pkgs.lixPackageSets.lix_2_93;
in
  pkgs.mkShell {
    name = "nix-bindings";
    packages = with pkgs; [
      cargo
      rustc

      rust-analyzer-unwrapped
      (rustfmt.override {asNightly = true;})
      clippy
      taplo
      lldb

      cargo-llvm-cov
    ];

    nativeBuildInputs = with pkgs; [
      nixForBindings.dev
      lixForBindings.lix.dev
      pkg-config
      glibc.dev
      #gcc
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
