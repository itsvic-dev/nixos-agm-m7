{ config, pkgs, modulesPath, ... }:

{
  imports = [
    "${modulesPath}/profiles/minimal.nix"
    ./kernel.nix
    ./rootfs.nix
    ./initrd.nix
    ./usb.nix
    ./sway.nix
    ./vendor.nix
    ./wifi.nix
    ./modem.nix
    ./firefox.nix
    ./gpu.nix
    ./settings.nix
    ./audio.nix
    ./sdcard.nix
  ];

  networking.hostName = "agm-m7";
  networking.useNetworkd = true;
  networking.useDHCP = false;
  networking.nftables.enable = true;

  services.getty.autologinUser = "root";

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
  # We deploy many times a day, so age alone keeps too many generations on the small root image.
  systemd.services.nix-gc.preStart = "${config.nix.package}/bin/nix-env -p /nix/var/nix/profiles/system --delete-generations +2";

  # The phone never evaluates nixpkgs, so it does not need its 200 MB source.
  nixpkgs.flake.setNixPath = false;
  nixpkgs.flake.setFlakeRegistry = false;

  environment.systemPackages = with pkgs; [
    evtest
    fastfetch
    i2c-tools
    libgpiod
    (runCommand "devmem" { } "mkdir -p $out/bin; ln -s ${busybox}/bin/busybox $out/bin/devmem")
  ];

  system.stateVersion = "26.05";
}
