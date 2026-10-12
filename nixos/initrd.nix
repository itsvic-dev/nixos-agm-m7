{
  lib,
  pkgs,
  kernel,
  ...
}:

let
  dmctl = pkgs.callPackage ../pkgs/dmctl.nix { };
in
{
  boot.kernelPackages = pkgs.linuxPackagesFor kernel;

  boot.kernelParams = [
    "earlycon=pstoreram,0x4445f000"
    "keep_bootcon"
    "mtk_wdt_early=0x10007000,30"
    "clk_ignore_unused"
    "regulator_ignore_unused"
    "g_cdc.dev_addr=02:4d:37:00:00:01"
    "g_cdc.host_addr=02:4d:37:00:00:02"
  ];

  boot.consoleLogLevel = 8;
  boot.initrd.systemd.enable = true;
  boot.initrd.systemd.services.emergency.serviceConfig.ExecStart = [
    ""
    "/bin/sh -c 'systemctl --failed --no-pager > /dev/kmsg; systemctl status --no-pager -l userdata.service run-userdata.mount sysroot.mount > /dev/kmsg 2>&1; journalctl -b --no-pager | tail -n 150 > /dev/kmsg; sleep 5; systemctl reboot -ff'"
  ];
  boot.initrd.compressor = "xz";
  boot.initrd.systemd.tpm2.enable = false;
  boot.initrd.systemd.fido2.enable = false;
  boot.initrd.services.lvm.enable = false;
  boot.initrd.includeDefaultModules = false;
  boot.initrd.availableKernelModules = lib.mkForce [ ];
  boot.initrd.kernelModules = lib.mkForce [ ];

  # /etc/userdata.table is appended by scripts/mkboot-nix.sh, outside the store.
  boot.initrd.systemd.extraBin.dmctl = "${dmctl}/bin/dmctl";

  boot.initrd.systemd.services.userdata = {
    wantedBy = [ "initrd-root-device.target" ];
    requires = [ "dev-mmcblk0p33.device" ];
    after = [ "dev-mmcblk0p33.device" ];
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      StandardOutput = "kmsg";
      StandardError = "kmsg";
    };
    script = ''
      ls -l /etc/userdata.table /dev/mmcblk0p33 /dev/mapper
      dmctl create userdata rw "$(cat /etc/userdata.table)"
    '';
  };

  boot.initrd.systemd.mounts = [
    {
      what = "/dev/dm-0";
      where = "/run/userdata";
      type = "f2fs";
      requires = [ "userdata.service" ];
      after = [ "userdata.service" ];
      wantedBy = [ "initrd-root-device.target" ];
      unitConfig.DefaultDependencies = false;
    }
  ];

  fileSystems."/" = {
    device = "/run/userdata/nixos.img";
    fsType = "ext4";
    options = [
      "loop"
      "x-systemd.requires-mounts-for=/run/userdata"
    ];
  };

  # dmctl creates the mapping in stage 1 without libdevmapper's udev handshake.
  services.udev.packages = [
    (pkgs.writeTextDir "lib/udev/rules.d/99-zz-userdata.rules" ''
      SUBSYSTEM=="block", KERNEL=="dm-*", ATTR{dm/name}=="userdata", ENV{DM_UDEV_DISABLE_OTHER_RULES_FLAG}="", ENV{SYSTEMD_READY}="1"
    '')
  ];

  fileSystems."/mnt/data" = {
    device = "/run/userdata";
    fsType = "none";
    options = [ "bind" ];
  };
}
