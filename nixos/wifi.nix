{ pkgs, ... }:

{
  # NetworkManager owns connections, routes and DNS; iwd is only its Wi-Fi backend.
  networking.wireless.iwd.enable = true;
  networking.networkmanager = {
    enable = true;
    wifi.backend = "iwd";
    unmanaged = [ "interface-name:usb0" ];
  };
  users.users.user.extraGroups = [ "networkmanager" ];

  environment.systemPackages = [ pkgs.iw ];

  # The Wi-Fi NVRAM is on /nvdata.
  systemd.services.wifi = {
    description = "Load the MT6739 Wi-Fi driver";
    wantedBy = [ "multi-user.target" ];
    wants = [ "nvdata.mount" ];
    after = [ "nvdata.mount" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${pkgs.kmod}/bin/modprobe mt6739-wlan";
    };
  };
}
