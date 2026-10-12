{ lib, ... }:

{
  boot.initrd.systemd.enable = true;
  # Reboot into LineageOS on failure, where ADB works, instead of looping.
  boot.initrd.systemd.services.emergency.serviceConfig.ExecStart = [
    ""
    "/bin/sh -c 'systemctl --failed --no-pager > /dev/kmsg; systemctl status --no-pager -l sysroot.mount > /dev/kmsg 2>&1; journalctl -b --no-pager | tail -n 150 > /dev/kmsg; sleep 5; systemctl reboot -ff --reboot-argument=recovery'"
  ];
  boot.initrd.compressor = "xz";
  boot.initrd.systemd.tpm2.enable = false;
  boot.initrd.systemd.fido2.enable = false;
  boot.initrd.services.lvm.enable = false;
  boot.initrd.includeDefaultModules = false;
  boot.initrd.availableKernelModules = lib.mkForce [ ];
  boot.initrd.kernelModules = lib.mkForce [ ];

  fileSystems."/" = {
    device = "/dev/disk/by-partlabel/nixos";
    fsType = "ext4";
    options = [ "x-systemd.device-timeout=15s" ];
  };
}
