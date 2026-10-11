{ pkgs, ... }:

{
  networking.wireless.iwd = {
    enable = true;
    settings.General.EnableNetworkConfiguration = true;
  };

  environment.systemPackages = [ pkgs.iw ];

  # The CONSYS ROM patches and the Wi-Fi firmware are on /vendor, the NVRAM on /nvdata.
  systemd.services.wifi = {
    description = "Load the MT6739 Wi-Fi driver";
    wantedBy = [ "multi-user.target" ];
    requires = [ "vendor.mount" ];
    wants = [ "nvdata.mount" ];
    after = [
      "vendor.mount"
      "nvdata.mount"
    ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${pkgs.kmod}/bin/modprobe mt6739-wlan";
    };
  };
}
