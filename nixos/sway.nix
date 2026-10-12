{ lib, pkgs, ... }:

let
  fuzzel = pkgs.fuzzel.override { svgBackend = "nanosvg"; };

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

    bindsym XF86Back exec pkill -RTMIN wvkbd-mobintl
    bindsym XF86MenuKB exec ${fuzzel}/bin/fuzzel
    bindsym XF86Send focus next
    bindsym XF86PowerOff kill

    bar {
      position top
      height 14
      font pango:DejaVu Sans Mono 7
      status_command ${status}
    }

    exec ${pkgs.wvkbd}/bin/wvkbd-mobintl --hidden -H 130
    exec ${pkgs.foot}/bin/foot
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

  programs.sway = {
    enable = true;
    xwayland.enable = false;
    extraPackages = [ ];
  };
  xdg.portal.wlr.enable = lib.mkForce false;
  fonts.enableDefaultPackages = lib.mkForce false;
  services.speechd.enable = lib.mkForce false;
  services.pipewire.enable = lib.mkForce false;

  environment.systemPackages = [
    pkgs.foot
    fuzzel
    pkgs.grim
    pkgs.wvkbd
  ];

  fonts.packages = [ pkgs.dejavu_fonts ];

  environment.etc."xdg/foot/foot.ini".text = ''
    font=DejaVu Sans Mono:size=6
  '';

  environment.etc."xdg/fuzzel/fuzzel.ini".text = ''
    [main]
    font=DejaVu Sans Mono:size=7
    width=30
    lines=12
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
