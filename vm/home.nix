{
  lib,
  pkgs,
  dotfiles,
  ...
}:
{
  # The desktop half of the dotfiles' home/default.nix. Its app list, agent
  # tooling and timers (home/services) stay out: they need credentials or
  # reach real services.
  imports = [
    "${dotfiles}/modules/home/fonts.nix"
    "${dotfiles}/modules/home/ghostty.nix"
    "${dotfiles}/modules/home/gtk.nix"
    "${dotfiles}/modules/home/stillsuit"
    "${dotfiles}/modules/home/stylix.nix"
    "${dotfiles}/home/desktop/wayland"
  ];

  # Declared by home/services/dictator.nix, which is not imported.
  options.dotfiles.services.dictator.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
  };

  config = {
    home.username = "kabilan";
    home.homeDirectory = "/home/kabilan";
    home.stateVersion = "25.11";
    programs.home-manager.enable = true;

    dotfiles.wallpaper.lockscreen = "$HOME/dotfiles/wallpapers/uwide/lucy.png";

    # Replaces hosts/sietch.kdl: one virtio-gpu output, and the Spindle rule.
    xdg.configFile."niri/host.kdl".source = lib.mkForce (
      pkgs.writeText "niri-host-spindle-vm.kdl" ''
        output "Virtual-1" {
            mode "1920x1080"
            scale 1
            position x=0 y=0
        }

        spawn-sh-at-startup "swaybg -i $HOME/dotfiles/wallpapers/uwide/lucy.png -m fill"

        ${builtins.readFile ./niri-spindle-rule.kdl}
      ''
    );

    home.packages = [ pkgs.playerctl ];
  };
}
