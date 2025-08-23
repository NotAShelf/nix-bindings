{pkgs ? import <nixpkgs> {}}: let
  nixForBindings = pkgs.nixVersions.nix_2_28;
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
    ];

    nativeBuildInputs = with pkgs; [
      nixForBindings.dev
      pkg-config
      glibc.dev
      #gcc
    ];

    env = {
      RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${pkgs.glibc.dev}";
    };
  }
