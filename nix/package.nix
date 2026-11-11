{
  lib,
  rustPlatform,
  pkg-config,
  makeWrapper,
  alsa-lib,
  fontconfig,
  libxkbcommon,
  vulkan-loader,
  wayland,
  libx11,
  libxcursor,
  libxi,
  libxrandr,
  libxcb,
}:
let
  # winit and wgpu open the display, keyboard and Vulkan libraries at run time.
  runtimeLibs = [
    alsa-lib
    fontconfig
    libxkbcommon
    vulkan-loader
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
    libxcb
  ];
in
rustPlatform.buildRustPackage {
  pname = "spindle";
  version = "0.1.0";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../assets
    ];
  };

  cargoLock = {
    lockFile = ../Cargo.lock;
    allowBuiltinFetchGit = true;
  };

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];
  buildInputs = runtimeLibs;

  cargoBuildFlags = [
    "--bin"
    "spindle"
  ];
  doCheck = false;

  postInstall = ''
    wrapProgram $out/bin/spindle --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
    install -Dm644 assets/spindle.desktop $out/share/applications/spindle.desktop
    install -Dm644 assets/spindle.svg $out/share/icons/hicolor/scalable/apps/spindle.svg
  '';

  passthru = { inherit runtimeLibs; };

  meta = {
    description = "A native Spotify client built with Rune";
    license = lib.licenses.mit;
    mainProgram = "spindle";
    platforms = lib.platforms.linux;
  };
}
