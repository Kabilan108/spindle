{
  description = "Headless QEMU VM running Tony's niri + stillsuit desktop, for driving Baliset from the host";

  # git+file reads the dotfiles working tree the way `nixos-rebuild --flake` does,
  # so the VM sees the same nixpkgs lock, niri, stillsuit and config.kdl.
  # Refresh after dotfiles changes with `nix flake update dotfiles`.
  inputs.dotfiles.url = "git+file:///home/kabilan/dotfiles";

  outputs =
    { self, dotfiles }:
    let
      system = "x86_64-linux";
      inputs = dotfiles.inputs;
      nixpkgs = inputs.nixpkgs;
      lib = nixpkgs.lib;

      # Same overlay list as makeSystem in the dotfiles flake.nix, so package
      # hashes match the real machine's and come out of the host store.
      overlays = [
        (final: prev: {
          ghostty = inputs.ghostty.packages.${final.stdenv.hostPlatform.system}.default;
          chatgpt-desktop = import "${dotfiles}/packages/chatgpt-desktop.nix" { pkgs = final; };
          grok-bot = import "${dotfiles}/packages/grok-bot.nix" { pkgs = final; };
          helium = import "${dotfiles}/packages/helium.nix" { pkgs = final; };
        })
        (import "${dotfiles}/packages/security-overrides.nix")
        inputs.pagebin.overlays.default
        inputs.atlas.overlays.default
        inputs.hark.overlays.default
        inputs.scout.overlays.default
        inputs.tracer.overlays.default
      ];

      pkgs = import nixpkgs {
        inherit system overlays;
        config.allowUnfree = true;
      };

      vm = lib.nixosSystem {
        specialArgs = {
          inherit inputs dotfiles;
        };
        modules = [
          {
            nixpkgs = {
              hostPlatform = system;
              config.allowUnfree = true;
              inherit overlays;
            };
          }
          "${nixpkgs}/nixos/modules/virtualisation/qemu-vm.nix"
          "${nixpkgs}/nixos/modules/profiles/qemu-guest.nix"
          inputs.home-manager.nixosModules.home-manager
          inputs.stylix.nixosModules.stylix
          "${dotfiles}/modules/nixos/theme.nix"
          "${dotfiles}/modules/nixos/desktop-wayland.nix"
          inputs."niri-flake".nixosModules.niri
          "${dotfiles}/modules/nixos/wayland/niri.nix"
          ./guest.nix
        ];
      };

      # Host-side tools the scripts in this directory run.
      tools = pkgs.buildEnv {
        name = "baliset-vm-tools";
        paths = [
          pkgs.python3
          pkgs.openssh
          pkgs.coreutils
          pkgs.procps
        ];
      };
    in
    {
      nixosConfigurations.baliset-vm = vm;
      packages.${system} = {
        vm = vm.config.system.build.vm;
        inherit tools;
        default = vm.config.system.build.vm;
      };
    };
}
