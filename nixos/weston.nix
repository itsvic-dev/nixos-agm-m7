{ pkgs, ... }:

let
  westonIni = pkgs.writeText "weston.ini" ''
    [core]
    shell=kiosk

    [autolaunch]
    path=${pkgs.foot}/bin/foot
    watch=true
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

  environment.etc."xdg/foot/foot.ini".text = ''
    font=monospace:size=7
  '';

  systemd.defaultUnit = "graphical.target";

  systemd.services.weston = {
    wantedBy = [ "graphical.target" ];
    after = [
      "systemd-user-sessions.service"
      "getty@tty1.service"
    ];
    conflicts = [ "getty@tty1.service" ];
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
      ExecStart = "${pkgs.weston}/bin/weston --backend=drm --renderer=pixman --config=${westonIni}";
    };
  };
}
