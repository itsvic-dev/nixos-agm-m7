{
  systemd.network.networks."10-usb0" = {
    matchConfig.Name = "usb0";
    address = [ "172.16.42.1/24" ];
    networkConfig.DHCPServer = true;
    dhcpServerConfig = {
      PoolOffset = 2;
      PoolSize = 1;
      EmitRouter = false;
      EmitDNS = false;
    };
  };
  networking.firewall.allowedUDPPorts = [ 67 ];

  systemd.services."serial-getty@ttyGS0" = {
    enable = true;
    wantedBy = [ "getty.target" ];
  };
}
