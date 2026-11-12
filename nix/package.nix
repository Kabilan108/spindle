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
  pname = "baliset";
  version = "0.1.0";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../vendor
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
    "baliset"
  ];
  doCheck = false;

  postInstall = ''
    wrapProgram $out/bin/baliset --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
    install -Dm644 assets/baliset.desktop $out/share/applications/baliset.desktop
    install -Dm644 assets/baliset.svg $out/share/icons/hicolor/scalable/apps/baliset.svg
  '';

  passthru = { inherit runtimeLibs; };

  meta = {
    description = "A native Spotify client built with Rune";
    license = lib.licenses.mit;
    mainProgram = "baliset";
    platforms = lib.platforms.linux;
  };
}
