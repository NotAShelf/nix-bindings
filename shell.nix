{pkgs ? import <nixpkgs> {}}: let
  nixForBindings = [
    pkgs.nixVersions.nix_2_28
    pkgs.nixVersions.nix_2_29
    pkgs.nixVersions.latest
  ];
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

    nativeBuildInputs =
      (with pkgs; [
        pkg-config
        glibc.dev
      ])
      ++ map (nix: nix.dev) nixForBindings;

    env = {
      RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${pkgs.glibc.dev}";
    };
  }
