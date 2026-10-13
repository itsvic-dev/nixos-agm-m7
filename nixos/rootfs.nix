{
  config,
  pkgs,
  modulesPath,
  ...
}:

let
  toplevel = config.system.build.toplevel;
in
{
  system.build.rootfs = pkgs.callPackage "${modulesPath}/../lib/make-ext4-fs.nix" {
    storePaths = [ toplevel ];
    volumeLabel = "nixos";
    populateImageCommands = ''
      mkdir -p files/sbin
      ln -s /nix/var/nix/profiles/system/init files/sbin/init
      mkdir -p files/nix/var/nix/profiles
      ln -s ${toplevel} files/nix/var/nix/profiles/system-1-link
      ln -s system-1-link files/nix/var/nix/profiles/system
    '';
  };

  system.build.sdImage = pkgs.runCommand "agm-m7-sd.img" { nativeBuildInputs = [ pkgs.util-linux ]; } ''
    root=${config.system.build.rootfs}
    size=$(stat -L -c %s $root)
    truncate -s $(( size + 2 * 1024 * 1024 )) $out
    echo "label: gpt
    label-id: 6E6F7869-6167-4D37-A000-000000000001
    start=2048, size=$(( size / 512 )), type=B921B045-1DF0-41C3-AF44-4C6F280D3FAE, uuid=6E6F7869-6167-4D37-A000-000000000002, name=nixos" | sfdisk -q $out
    dd if=$root of=$out bs=1M seek=1 conv=notrunc status=none
  '';

  fileSystems."/mnt/sd" = {
    device = "/dev/disk/by-partlabel/nixos";
    fsType = "ext4";
    options = [
      "nofail"
      "x-systemd.device-timeout=15s"
    ];
  };
  fileSystems."/home" = {
    device = "/mnt/sd/home";
    fsType = "none";
    options = [
      "bind"
      "nofail"
    ];
    depends = [ "/mnt/sd" ];
  };

  services.fstrim.enable = true;

  systemd.services.nix-register-rootfs = {
    wantedBy = [ "multi-user.target" ];
    before = [
      "nix-daemon.socket"
      "nix-daemon.service"
      "shutdown.target"
    ];
    after = [ "local-fs.target" ];
    conflicts = [ "shutdown.target" ];
    unitConfig = {
      DefaultDependencies = false;
      ConditionPathExists = "/nix-path-registration";
    };
    serviceConfig.Type = "oneshot";
    script = ''
      ${config.nix.package.out}/bin/nix-store --load-db < /nix-path-registration
      touch /etc/NIXOS
      ${config.nix.package.out}/bin/nix-env -p /nix/var/nix/profiles/system --set /run/current-system
      rm -f /nix-path-registration
    '';
  };
}
