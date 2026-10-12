{
  config,
  lib,
  pkgs,
  ...
}:

let
  t9 = pkgs.callPackage ../tools/t9/package.nix { sway = config.programs.sway.package; };
  shell = pkgs.callPackage ../tools/shell/package.nix { sway = config.programs.sway.package; };

  bindings = ''
    bindsym XF86MenuKB exec ${shell}/bin/agm-menu --hide footclient.desktop --hide foot-server.desktop
    bindsym XF86Send focus next
    bindsym F11 exec pkill -RTMIN wvkbd-mobintl
    bindsym XF86PowerOff kill
  '';

  brightnessctl = "${pkgs.brightnessctl}/bin/brightnessctl";
  swaymsg = "${config.programs.sway.package}/bin/swaymsg";

  # Save only a brighter level, so a restore never brings back the dim one.
  dim = pkgs.writeShellScript "sway-dim" ''
    level=$(( $(${brightnessctl} m) * 30 / 100 ))
    if [ "$(${brightnessctl} g)" -gt "$level" ]; then
      ${brightnessctl} -s set "$level"
    fi
  '';

  # The first "power on" after "power off" fails in wlroots without an error; the second one works.
  idle = pkgs.writeShellScript "sway-idle" ''
    # Sway may have restarted while the screen was dimmed.
    ${brightnessctl} -r || true
    conf="''${XDG_CONFIG_HOME:-$HOME/.config}/agm/settings"
    get() { sed -n "s/^$1=//p" "$conf" 2>/dev/null | grep . || echo "$2"; }
    while :; do
      dim=$(get dim_after 20)
      off=$(get off_after 30)
      set --
      [ "$dim" != 0 ] && set -- "$@" timeout "$dim" '${dim}' resume '${brightnessctl} -r'
      [ "$off" != 0 ] && set -- "$@" timeout "$off" '${shell}/bin/agm-lock & ${swaymsg} "output * power off"' \
        resume '${swaymsg} "output * power on"; ${swaymsg} "output * power on"'
      if [ $# -eq 0 ]; then
        sleep infinity &
        wait $!
      else
        ${pkgs.swayidle}/bin/swayidle -w "$@"
      fi
      ${brightnessctl} -r || true
      sleep 1
    done
  '';

  status = pkgs.writeShellScript "sway-status" ''
    while :; do
      echo "$(cat /sys/class/power_supply/battery/capacity)%  $(date +%H:%M)"
      sleep 30
    done
  '';

  swayConfig = pkgs.writeText "sway-config" ''
    font pango:DejaVu Sans Mono 7
    output * bg #1e1e2e solid_color
    default_border pixel 1
    workspace_layout tabbed
    focus_follows_mouse no

    ${bindings}

    # agm-menu switches to this mode, so it gets all keys. Sway drops empty modes.
    mode menu {
      bindsym XF86Send nop
    }

    # t9 switches to this mode while a text field has focus, so it gets the right soft key.
    mode text {
      ${bindings}
    }

    bar {
      position top
      height 14
      font pango:DejaVu Sans Mono 7
      status_command ${status}
    }

    exec ${shell}/bin/agm-home
    exec ${idle}
    exec ${pkgs.wvkbd}/bin/wvkbd-mobintl --hidden -H 130
    exec ${t9}/bin/t9
  '';
in
{
  users.users.user = {
    isNormalUser = true;
    extraGroups = [
      "video"
      "input"
    ];
  };

  nixpkgs.overlays = [
    (final: prev: {
      sway-unwrapped = prev.sway-unwrapped.overrideAttrs (old: {
        patches = (old.patches or [ ]) ++ [ ./patches/sway-im-popup-no-text-input.patch ];
      });
    })
  ];

  programs.sway = {
    enable = true;
    xwayland.enable = false;
    extraPackages = [ ];
  };
  xdg.portal.wlr.enable = lib.mkForce false;
  fonts.enableDefaultPackages = lib.mkForce false;
  services.speechd.enable = lib.mkForce false;
  services.pipewire.enable = lib.mkForce false;

  services.udev.packages = [ pkgs.brightnessctl ];

  # iwd access for the home screen and the settings app, without wheel.
  services.dbus.packages = [
    (pkgs.writeTextDir "share/dbus-1/system.d/agm-shell-iwd.conf" ''
      <!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
        "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
      <busconfig>
        <policy user="user">
          <allow send_destination="net.connman.iwd"/>
        </policy>
      </busconfig>
    '')
  ];

  environment.systemPackages = [
    shell
    pkgs.brightnessctl
    pkgs.foot
    pkgs.grim
    pkgs.wvkbd
  ];

  fonts.packages = [ pkgs.dejavu_fonts ];

  environment.etc."xdg/foot/foot.ini".text = ''
    font=DejaVu Sans Mono:size=6
  '';

  # The hang-up key is KEY_POWER; Sway uses it to close windows.
  services.logind.settings.Login.HandlePowerKey = "ignore";

  systemd.defaultUnit = "graphical.target";
  systemd.services."getty@tty1".enable = false;
  systemd.services."autovt@tty1".enable = false;

  systemd.services.sway = {
    wantedBy = [ "graphical.target" ];
    restartIfChanged = false;
    after = [
      "systemd-user-sessions.service"
      "getty@tty1.service"
    ];
    conflicts = [
      "getty@tty1.service"
      "autovt@tty1.service"
    ];
    environment.WLR_RENDERER = "pixman";
    path = [
      pkgs.bash
      pkgs.procps
    ];
    serviceConfig = {
      User = "user";
      PAMName = "login";
      TTYPath = "/dev/tty1";
      TTYReset = true;
      TTYVHangup = true;
      TTYVTDisallocate = true;
      StandardInput = "tty-fail";
      StandardOutput = "journal";
      StandardError = "journal";
      UtmpIdentifier = "tty1";
      UtmpMode = "user";
      Restart = "always";
      ExecStart = "/run/current-system/sw/bin/sway --config ${swayConfig}";
    };
  };
}
