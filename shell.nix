{pkgs ? import <nixpkgs> {}}:
with pkgs;
  mkShell {
    packages = [
      cargo
      rustc

      rust-analyzer-unwrapped
      (rustfmt.override {asNightly = true;})
      clippy
      nix-output-monitor
      taplo
      yaml-language-server
      lldb
    ];

    nativeBuildInputs = [
      nix.dev
      pkg-config
      glibc.dev
      gcc
    ];

    env = {
      RUST_SRC_PATH = "${rustPlatform.rustLibSrc}";
      LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
      BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${pkgs.glibc.dev}";
    };
  }
