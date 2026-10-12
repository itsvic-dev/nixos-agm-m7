{
  stdenv,
  runCommand,
  writeText,
  makeFontsConf,
  pkg-config,
  wayland-scanner,
  wayland,
  libxkbcommon,
  dejavu_fonts,
  sway-unwrapped,
  foot,
  grim,
  t9,
}:

let
  send = stdenv.mkDerivation {
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
  };

  config = writeText "sway-test-config" ''
    output HEADLESS-1 resolution 240x320
    font pango:DejaVu Sans Mono 7
    default_border pixel 1
    mode text {
      bindsym XF86Send focus next
    }
    exec ${t9}/bin/t9
  '';

  fonts = makeFontsConf { fontDirectories = [ dejavu_fonts ]; };

  KEY_ENTER = "28";
  KEY_UP = "103";
  KEY_DOWN = "108";
  KEY_BACK = "158";
  KEY_STAR = "522";
  KEY_POUND = "523";
in
runCommand "t9-wayland-test"
  {
    nativeBuildInputs = [
      sway-unwrapped
      foot
      grim
      send
    ];
  }
  ''
    export HOME=$TMPDIR XDG_RUNTIME_DIR=$TMPDIR/run FONTCONFIG_FILE=${fonts} LANG=C.UTF-8
    export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
    mkdir -m 700 $XDG_RUNTIME_DIR
    mkdir $out

    sway -c ${config} > $out/sway.log 2>&1 &
    for i in $(seq 50); do
      [ -S $XDG_RUNTIME_DIR/wayland-1 ] && break
      sleep 0.2
    done
    export WAYLAND_DISPLAY=wayland-1

    foot -- sh -c "IFS= read -r line; printf '%s' \"\$line\" > $out/typed.txt" > $out/foot.log 2>&1 &
    sleep 3

    # Digit keys: KEY_1 is 2, KEY_9 is 10, KEY_0 is 11.
    t9-send 5 7 7 4
    grim $out/1-good.png
    t9-send ${KEY_STAR} 11 9 5 4 ${KEY_DOWN} ${KEY_UP} 2 11 5 7 7 4
    grim $out/2-sentence.png
    t9-send 6:l ${KEY_BACK} ${KEY_POUND}
    grim $out/3-mode.png
    t9-send ${KEY_POUND} 11 3 3
    sleep 1.5
    t9-send ${KEY_ENTER}
    sleep 1

    typed=$(cat $out/typed.txt 2>/dev/null || true)
    if [ "$typed" != "Home the. Good b" ]; then
      echo "typed: '$typed'"
      cat $out/*.log
      exit 1
    fi
  ''
