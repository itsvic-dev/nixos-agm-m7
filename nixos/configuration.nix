{
  config,
  lib,
  pkgs,
  modulesPath,
  ...
}:

{
  imports = [
    "${modulesPath}/profiles/minimal.nix"
    ./rootfs.nix
    ./initrd.nix
    ./sway.nix
    ./vendor.nix
    ./wifi.nix
    ./firefox.nix
    ./gpu.nix
  ];

  boot.loader.grub.enable = false;
  boot.loader.initScript.enable = false;
  # Built-in drivers only, so /proc/sys/kernel/modprobe does not exist.
  system.activationScripts.modprobe = lib.mkForce "";
  # Loaded by hand while the DDK 1.9 GPU firmware path is in development.
  boot.blacklistedKernelModules = [ "powervr" ];


  networking.hostName = "agm-m7";
  time.timeZone = "Europe/Warsaw";
  networking.useNetworkd = true;
  networking.useDHCP = false;
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
  networking.nftables.enable = true;
  networking.firewall.allowedUDPPorts = [ 67 ];

  services.getty.autologinUser = "root";
  systemd.services."serial-getty@ttyGS0" = {
    enable = true;
    wantedBy = [ "getty.target" ];
  };

  services.openssh.enable = true;
  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPQ9kubQPsQKJdtOLG35DApD9IWNU5gR6Dm+e2cy2L83"
  ];

  nix.settings = {
    experimental-features = [
      "nix-command"
      "flakes"
    ];
    min-free = 300 * 1024 * 1024;
    max-free = 800 * 1024 * 1024;
  };
  nix.gc = {
    automatic = true;
    dates = "daily";
    options = "--delete-older-than 3d";
  };

  environment.systemPackages = with pkgs; [
    evtest
    fastfetch
    i2c-tools
    libgpiod
    (runCommand "devmem" { } "mkdir -p $out/bin; ln -s ${busybox}/bin/busybox $out/bin/devmem")
  ];

  system.stateVersion = "26.05";
}
