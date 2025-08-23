{
  inputs.nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.05";
  outputs = {nixpkgs, ...}: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forEachSystem = nixpkgs.lib.genAttrs systems;
    pkgsForEach = nixpkgs.legacyPackages;
  in {
    devShells = forEachSystem (system: let
      pkgs = pkgsForEach.${system};
    in {
      default = pkgsForEach.${system}.callPackage ./shell.nix {
        inherit pkgs;
      };
    });
  };
}
