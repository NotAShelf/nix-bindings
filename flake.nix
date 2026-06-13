{
  inputs.nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";

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
