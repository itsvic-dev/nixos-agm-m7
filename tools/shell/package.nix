{
  lib,
  rustPlatform,
  runCommand,
  pkg-config,
  imagemagick,
  libxkbcommon,
  dejavu_fonts,
  nixos-artwork,
  sway,
  foot,
}:

let
  wallpaper = runCommand "agm-wallpaper.png" { nativeBuildInputs = [ imagemagick ]; } ''
    magick ${nixos-artwork.wallpapers.nineish-catppuccin-mocha}/share/artwork/gnome/nix-wallpaper-nineish-catppuccin-mocha.png \
      -resize 240x320^ -gravity center -extent 240x320 $out
  '';
  fonts = "${dejavu_fonts}/share/fonts/truetype";
in
rustPlatform.buildRustPackage {
  pname = "agm-shell";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ libxkbcommon ];

  AGM_CLOCK_FONT = "${fonts}/DejaVuSans-ExtraLight.ttf";
  AGM_TEXT_FONT = "${fonts}/DejaVuSans.ttf";
  AGM_WALLPAPER = wallpaper;
  AGM_SWAYMSG = "${sway}/bin/swaymsg";
  AGM_TERMINAL = "${foot}/bin/foot";
}
