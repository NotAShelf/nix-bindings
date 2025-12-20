{
  # We generally want to target the stable branch of NixOS, because it allows
  # us to infer a "minimum stable release" to support. Namely, it allows for
  # having a reference point, so we know what is ready for public use and what
  # is not. Security and critical performance improvements are always included
  # so it makes a good starting point.
  inputs.nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";

  outputs = {nixpkgs, ...}: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forEachSystem = nixpkgs.lib.genAttrs systems;
    pkgsForEach = nixpkgs.legacyPackages;
  in {
    devShells = forEachSystem (system: let
      pkgs = pkgsForEach.${system};
    in {
      default = pkgsForEach.${system}.callPackage ./shell.nix {inherit pkgs;};
    });
  };
}
