{
  stdenv,
  pkg-config,
  wayland-scanner,
  wayland,
  libxkbcommon,
}:

stdenv.mkDerivation {
  name = "t9-send";
  src = ../.;
  nativeBuildInputs = [
    pkg-config
    wayland-scanner
  ];
  buildInputs = [
    wayland
    libxkbcommon
  ];
  buildPhase = ''
    wayland-scanner client-header protocol/virtual-keyboard-unstable-v1.xml virtual-keyboard-unstable-v1-client-protocol.h
    wayland-scanner private-code protocol/virtual-keyboard-unstable-v1.xml vk-protocol.c
    $CC -O2 -Wall -I. -o t9-send test/send.c vk-protocol.c $(pkg-config --cflags --libs wayland-client xkbcommon)
  '';
  installPhase = "install -Dm755 t9-send $out/bin/t9-send";
}
