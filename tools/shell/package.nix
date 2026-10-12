{
  lib,
  rustPlatform,
  runCommand,
  pkg-config,
  imagemagick,
  python3Packages,
  libxkbcommon,
  inter,
  adwaita-icon-theme,
  nixos-artwork,
  sway,
  foot,
  pipewire,
  wireplumber,
}:

let
  wallpaper = runCommand "agm-wallpaper.png" { nativeBuildInputs = [ imagemagick ]; } ''
    magick ${nixos-artwork.wallpapers.nineish-catppuccin-mocha}/share/artwork/gnome/nix-wallpaper-nineish-catppuccin-mocha.png \
      -background black -alpha remove -alpha off -resize 240x320^ -gravity center -extent 240x320 \
      -colorspace gray -normalize +level 7%,20% -type TrueColor $out
  '';

  # Static instances of the variable font: text sizes use opsz 14, the big clock opsz 32.
  fonts = runCommand "agm-fonts" { nativeBuildInputs = [ python3Packages.fonttools ]; } ''
    mkdir $out
    font=${inter}/share/fonts/truetype/InterVariable.ttf
    fonttools varLib.instancer -q $font wght=400 opsz=14 -o $out/Inter-Regular.ttf
    fonttools varLib.instancer -q $font wght=500 opsz=14 -o $out/Inter-Medium.ttf
    fonttools varLib.instancer -q $font wght=600 opsz=14 -o $out/Inter-SemiBold.ttf
    fonttools varLib.instancer -q $font wght=300 opsz=32 -o $out/Inter-Display-Light.ttf
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
      ./icons
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ libxkbcommon ];

  AGM_FONT_DIR = fonts;
  AGM_ICON_DIR = "${adwaita-icon-theme}/share";
  AGM_WALLPAPER = wallpaper;
  AGM_SWAYMSG = "${sway}/bin/swaymsg";
  AGM_TERMINAL = "${foot}/bin/foot";
  AGM_WPCTL = "${wireplumber}/bin/wpctl";
  AGM_PW_DUMP = "${pipewire}/bin/pw-dump";

  postInstall = ''
    install -Dm644 icons/agm-settings.svg $out/share/icons/hicolor/scalable/apps/agm-settings.svg
    install -Dm644 /dev/stdin $out/share/applications/agm-settings.desktop <<EOF
    [Desktop Entry]
    Type=Application
    Name=Settings
    Exec=$out/bin/agm-settings
    Icon=agm-settings
    EOF
  '';

  passthru = { inherit fonts; };
}
