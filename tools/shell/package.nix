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

  settingsIcon = runCommand "agm-settings.png" { nativeBuildInputs = [ imagemagick ]; } ''
    magick -size 64x64 xc:none -fill '#cba6f7' -draw 'roundrectangle 0,0 63,63 14,14' \
      -font ${fonts}/DejaVuSans.ttf -pointsize 50 -fill '#1e1e2e' -gravity center -annotate +0+1 '⚙' $out
  '';
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

  postInstall = ''
    install -Dm644 ${settingsIcon} $out/share/icons/hicolor/64x64/apps/agm-settings.png
    install -Dm644 /dev/stdin $out/share/applications/agm-settings.desktop <<EOF
    [Desktop Entry]
    Type=Application
    Name=Settings
    Exec=$out/bin/agm-settings
    Icon=agm-settings
    EOF
  '';
}
