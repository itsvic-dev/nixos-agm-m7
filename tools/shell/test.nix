{
  callPackage,
  runCommand,
  writeText,
  makeFontsConf,
  dejavu_fonts,
  sway-unwrapped,
  grim,
  foot,
  agm-shell,
}:

let
  send = callPackage ../t9/test/send.nix { };

  config = writeText "sway-test-config" ''
    output HEADLESS-1 resolution 240x320
    font pango:DejaVu Sans Mono 7
    bar {
      position top
      height 14
      font pango:DejaVu Sans Mono 7
      status_command echo "87%  12:34"
    }
    exec ${agm-shell}/bin/agm-home
  '';

  fonts = makeFontsConf { fontDirectories = [ dejavu_fonts ]; };

  KEY_MENU = "139";
  KEY_1 = "2";
  KEY_STAR = "522";
  KEY_RIGHT = "106";
  KEY_BACK = "158";

  # One entry without an icon, to show the letter tile.
  apps = runCommand "agm-shell-test-apps" { } ''
    mkdir -p $out/share/applications
    cat > $out/share/applications/settings.desktop <<EOF
    [Desktop Entry]
    Type=Application
    Name=Settings
    Exec=true
    EOF
  '';
in
runCommand "agm-shell-test"
  {
    nativeBuildInputs = [
      sway-unwrapped
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
    sleep 2
    grim $out/home.png

    ${agm-shell}/bin/agm-lock > $out/lock.log 2>&1 &
    lock=$!
    sleep 1
    grim $out/lock.png

    # Each t9-send call makes a new virtual keyboard, and its first key is lost.
    t9-send ${KEY_1} ${KEY_MENU} ${KEY_1} ${KEY_STAR}
    sleep 0.5
    kill -0 $lock

    t9-send ${KEY_1} ${KEY_MENU}
    sleep 0.3
    grim $out/lock-menu.png
    t9-send ${KEY_1} ${KEY_MENU} ${KEY_STAR}
    sleep 1
    if kill -0 $lock 2>/dev/null; then
      echo "agm-lock is still running after Menu, *"
      cat $out/*.log
      exit 1
    fi
    wait $lock
    grim $out/unlocked.png

    export XDG_DATA_DIRS=${foot}/share:${apps}/share
    ${agm-shell}/bin/agm-menu --hide footclient.desktop --hide foot-server.desktop > $out/menu.log 2>&1 &
    menu=$!
    sleep 1
    grim $out/menu.png
    t9-send ${KEY_1} ${KEY_RIGHT}
    sleep 0.3
    grim $out/menu-right.png
    t9-send ${KEY_1} ${KEY_BACK}
    sleep 1
    if kill -0 $menu 2>/dev/null; then
      echo "agm-menu is still running after Back"
      cat $out/*.log
      exit 1
    fi
  ''
