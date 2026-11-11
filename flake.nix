{
  description = "Spindle: a native Spotify client built with Rune";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs [
          "x86_64-linux"
          "aarch64-linux"
        ] (system: f nixpkgs.legacyPackages.${system});
    in
    {
      overlays.default = final: _: {
        spindle = final.callPackage ./nix/package.nix { };
      };

      packages = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/package.nix { };
      });

      devShells = forAllSystems (
        pkgs:
        let
          spindle = pkgs.callPackage ./nix/package.nix { };
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
              pkg-config
            ];
            buildInputs = spindle.runtimeLibs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath spindle.runtimeLibs;
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        }
      );
    };
}
