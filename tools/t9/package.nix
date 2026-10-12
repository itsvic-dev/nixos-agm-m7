{
  stdenv,
  fetchurl,
  pkg-config,
  wayland-scanner,
  wayland,
  cairo,
  pango,
  procps,
  sway,
}:

let
  words =
    lang: hash:
    fetchurl {
      url = "https://raw.githubusercontent.com/hermitdave/FrequencyWords/525f9b560de45753a5ea01069454e72e9aa541c6/content/2018/${lang}/${lang}_50k.txt";
      inherit hash;
    };
  en = words "en" "sha256-U1H/QFsRJu9VV5HdTZeYpI4+mlAan8SBqdqVd1LPtFg=";
  pl = words "pl" "sha256-UL+NMNHtT1ztzrPkDnTXk+xr7A8W2fPdCiyeMTQC4Q8=";
in
stdenv.mkDerivation {
  name = "t9";
  src = ./.;

  nativeBuildInputs = [
    pkg-config
    wayland-scanner
  ];
  buildInputs = [
    wayland
    cairo
    pango
  ];

  buildPhase = ''
    for p in input-method-unstable-v2 virtual-keyboard-unstable-v1; do
      wayland-scanner client-header protocol/$p.xml $p-client-protocol.h
      wayland-scanner private-code protocol/$p.xml $p-protocol.c
    done
    $CC -O2 -Wall -o t9 main.c engine.c *-protocol.c \
      -DT9_DATADIR=\"$out/share/t9\" \
      -DSWAYMSG=\"${sway}/bin/swaymsg\" \
      -DPKILL=\"${procps}/bin/pkill\" \
      $(pkg-config --cflags --libs wayland-client pangocairo)
    $CC -O2 -Wall -o t9-test engine.c test.c
  '';

  doCheck = true;
  checkPhase = "./t9-test";

  installPhase = ''
    install -Dm755 t9 $out/bin/t9
    mkdir -p $out/share/t9
    cat ${en} en-extra.txt > $out/share/t9/en.txt
    cp ${pl} $out/share/t9/pl.txt
  '';
}
