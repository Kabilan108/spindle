{
  config,
  lib,
  pkgs,
  inputs,
  dotfiles,
  ...
}:
let
  projectDir = "/home/kabilan/.t3/scratch/2026-11-10-i-want-you-to-build-cb0d1b12";

  mpvWithMpris = pkgs.mpv.override { scripts = [ pkgs.mpvScripts.mpris ]; };

  # Runs a command with the graphical session's environment, the way an app
  # launched from niri would see it. Used by the host's vm/run over SSH.
  sessionExec = pkgs.writeShellScriptBin "vm-session-exec" ''
    set -euo pipefail
    export XDG_RUNTIME_DIR="/run/user/$(id -u)"
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
    set -a
    eval "$(systemctl --user show-environment)"
    set +a
    if [ -z "''${WAYLAND_DISPLAY:-}" ]; then
      echo "vm-session-exec: niri has not exported WAYLAND_DISPLAY yet" >&2
      exit 75
    fi
    exec "$@"
  '';
in
{
  networking.hostName = "baliset-vm";
  system.stateVersion = "25.11";

  virtualisation = {
    memorySize = 8192;
    cores = 8;
    diskSize = 8192;
    # No window on the host; the guest still gets a virtio-gpu display that
    # niri drives and QMP screendump reads.
    graphics = false;
    qemu.options = [
      "-vga none"
      "-device virtio-gpu-pci,xres=1920,yres=1080"
    ];
    sharedDirectories.project = {
      source = ''''${BALISET_PROJECT_DIR:-${projectDir}}'';
      target = projectDir;
      writable = false;
    };
  };

  boot.kernelParams = [ "quiet" ];

  # niri's tty backend refuses software EGL (llvmpipe) outright, and the VM
  # has no GPU. Same niri revision and config, with that check removed. A
  # software EGL device reports no render node, so fall back to the card's
  # render node, which is what niri picked as its primary renderer.
  programs.niri.package = lib.mkForce (
    pkgs.niri-unstable.overrideAttrs (old: {
      postPatch = (old.postPatch or "") + ''
        substituteInPlace src/backend/tty.rs \
          --replace-fail '!egl_device.is_software(),' 'true,' \
          --replace-fail '.unwrap_or(node);' \
            '.unwrap_or_else(|| node.node_with_type(NodeType::Render).and_then(Result::ok).unwrap_or(node));'
      '';
    })
  );

  # Throwaway key from vm/up, passed as an SMBIOS systemd credential.
  systemd.services.vm-authorized-keys = {
    description = "Install the host's throwaway SSH key for kabilan";
    wantedBy = [ "multi-user.target" ];
    before = [ "sshd.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ImportCredential = "vm.ssh.authorized_keys";
    };
    script = ''
      install -d -m 0755 /run/vm-ssh
      if [ -f "$CREDENTIALS_DIRECTORY/vm.ssh.authorized_keys" ]; then
        install -m 0644 "$CREDENTIALS_DIRECTORY/vm.ssh.authorized_keys" /run/vm-ssh/authorized_keys
      fi
    '';
  };

  services.openssh = {
    enable = true;
    authorizedKeysFiles = [ "/run/vm-ssh/authorized_keys" ];
    settings = {
      PasswordAuthentication = false;
      KbdInteractiveAuthentication = false;
      PermitRootLogin = "no";
    };
  };

  users.mutableUsers = false;
  users.users.kabilan = {
    isNormalUser = true;
    uid = 1000;
    description = "Tony Kabilan Okeke";
    extraGroups = [
      "wheel"
      "video"
      "input"
      "audio"
      "networkmanager"
    ];
    shell = pkgs.bashInteractive;
    hashedPassword = "";
    linger = true;
  };
  security.sudo.wheelNeedsPassword = false;
  services.getty.autologinUser = "kabilan";

  # The real machine logs in on tty1 and runs niri-session; greetd does the
  # same here without a prompt, and restarts the session if niri exits.
  services.greetd = {
    enable = true;
    settings = rec {
      initial_session = {
        command = "niri-session";
        user = "kabilan";
      };
      default_session = initial_session;
    };
  };

  # The dotfiles' niri config and stillsuit plugin roots point at
  # ~/dotfiles; give them the pinned flake source.
  systemd.tmpfiles.rules = [
    "L+ /home/kabilan/dotfiles - - - - ${dotfiles}"
    "d /home/kabilan/media 0755 kabilan users -"
    "d /home/kabilan/media/screenshots 0755 kabilan users -"
  ];

  hardware.graphics.enable = true;
  environment.sessionVariables = {
    # Mesa lavapipe is the only Vulkan device; name it so the loader never
    # goes looking for others.
    VK_DRIVER_FILES = "/run/opengl-driver/share/vulkan/icd.d/lvp_icd.x86_64.json";
  };

  # Same audio stack as configuration.nix, plus a null sink so the default
  # ALSA/Pulse device exists without a sound card.
  security.rtkit.enable = true;
  services.pulseaudio.enable = false;
  services.pipewire = {
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
    extraConfig.pipewire."90-vm-null-sink" = {
      "context.objects" = [
        {
          factory = "adapter";
          args = {
            "factory.name" = "support.null-audio-sink";
            "node.name" = "vm-null-sink";
            "node.description" = "VM Null Output";
            "media.class" = "Audio/Sink";
            "audio.position" = "FL,FR";
            "priority.session" = 2000;
          };
        }
      ];
    };
  };

  networking.networkmanager.enable = true;
  services.upower.enable = true;
  services.dbus.packages = [ pkgs.dconf ];
  programs.dconf.enable = true;

  time.timeZone = "America/New_York";
  i18n.defaultLocale = "en_US.UTF-8";

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  environment.systemPackages = [
    sessionExec
    mpvWithMpris
    pkgs.playerctl
    pkgs.vulkan-tools
    pkgs.mesa-demos
    pkgs.foot
    # Tray stand-in. Not installed as a package: its XDG autostart entry would
    # put a second tray item up at every login.
    (pkgs.writeShellScriptBin "vm-tray-standin" ''exec ${lib.getExe pkgs.pasystray} "$@"'')
    pkgs.pulseaudio
    pkgs.alsa-utils
    pkgs.grim
    pkgs.wtype
    pkgs.jq
    pkgs.nirius
    pkgs.git
  ];

  home-manager = {
    useGlobalPkgs = true;
    useUserPackages = true;
    extraSpecialArgs = {
      inherit inputs dotfiles;
    };
    users.kabilan.imports = [ ./home.nix ];
  };
}
