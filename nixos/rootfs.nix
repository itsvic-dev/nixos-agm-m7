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

  # Root partition type root-arm64, which systemd-repart matches as Type=root.
  system.build.sdImage = pkgs.runCommand "agm-m7-sd.img" { nativeBuildInputs = [ pkgs.util-linux ]; } ''
    root=${config.system.build.rootfs}
    size=$(stat -L -c %s $root)
    truncate -s $(( size + 2 * 1024 * 1024 )) $out
    echo "label: gpt
    label-id: 6E6F7869-6167-4D37-A000-000000000001
    start=2048, size=$(( size / 512 )), type=B921B045-1DF0-41C3-AF44-4C6F280D3FAE, uuid=6E6F7869-6167-4D37-A000-000000000002, name=nixos" | sfdisk -q $out
    dd if=$root of=$out bs=1M seek=1 conv=notrunc status=none
  '';

  # On the first boot, grow the root partition and its ext4 to the whole card.
  systemd.repart.enable = true;
  systemd.repart.partitions."10-root".Type = "root";

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
