{ pkgs, ... }:

{
  users.users.user = {
    isNormalUser = true;
    extraGroups = [
      "video"
      "input"
    ];
  };

  environment.systemPackages = with pkgs; [
    foot
    wvkbd
  ];

  fonts.packages = [ pkgs.dejavu_fonts ];

  environment.etc."xdg/foot/foot.ini".text = ''
    font=DejaVu Sans Mono:size=6
  '';

  environment.etc."xdg/labwc/autostart".text = ''
    ${pkgs.wvkbd}/bin/wvkbd-mobintl -H 130 &
    ${pkgs.foot}/bin/foot &
  '';

  environment.etc."xdg/labwc/rc.xml".text = ''
    <?xml version="1.0"?>
    <labwc_config>
      <core><decoration>client</decoration></core>
      <windowRules>
        <windowRule identifier="foot" serverDecoration="no">
          <action name="Maximize"/>
        </windowRule>
      </windowRules>
    </labwc_config>
  '';

  systemd.defaultUnit = "graphical.target";

  systemd.services.labwc = {
    wantedBy = [ "graphical.target" ];
    after = [
      "systemd-user-sessions.service"
      "getty@tty1.service"
    ];
    conflicts = [ "getty@tty1.service" ];
    environment.WLR_RENDERER = "pixman";
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
      ExecStart = "${pkgs.labwc}/bin/labwc";
    };
  };
}
