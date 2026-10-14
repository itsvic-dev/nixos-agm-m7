{
  config,
  lib,
  pkgs,
  modulesPath,
  ...
}:

let
  cfg = config.boot.loader.agm-m7;
  bootParts = pkgs.callPackage ../pkgs/boot-parts.nix {
    inherit (config.system.build) kernel;
    initrd = config.system.build.initialRamdisk;
    inherit (config.boot) kernelParams;
  };
  bootImage = pkgs.callPackage ../pkgs/boot-img.nix { inherit bootParts; };
  extlinuxBuilder = import "${modulesPath}/system/boot/loader/generic-extlinux-compatible/extlinux-conf-builder.nix" {
    inherit lib pkgs;
  };
in
{
  options.boot.loader.agm-m7 = {
    enable = lib.mkEnableOption "the AGM M7 boot loader setup (U-Boot extlinux, LK boot.img)";
    configurationLimit = lib.mkOption {
      type = lib.types.ints.positive;
      default = 8;
      description = "Number of generations in extlinux.conf.";
    };
    device = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = "/dev/disk/by-partlabel/boot";
      description = "Partition that LK boots boot.img from, or null when U-Boot is there.";
    };
  };

  config = lib.mkIf cfg.enable {
    boot.loader.grub.enable = false;
    boot.loader.initScript.enable = false;

    # U-Boot reads extlinux.conf from the FAT file system on the eMMC vendor partition.
    fileSystems."/boot" = {
      device = "/dev/disk/by-partlabel/vendor";
      fsType = "vfat";
      options = [
        "umask=0077"
        "nofail"
      ];
    };

    system.build = {
      inherit bootParts bootImage;
      installBootLoader = pkgs.writeShellScript "install-agm-m7-boot-loader" (
        ''
          set -euo pipefail
          export PATH=${
            lib.makeBinPath [
              pkgs.coreutils
              pkgs.diffutils
              pkgs.util-linux
            ]
          }
          if ! mountpoint -q /boot; then
            echo "/boot is not mounted" >&2
            exit 1
          fi
          # -t -1: TIMEOUT 0, so sysboot boots the default at once and sysboot -p waits for a choice.
          ${extlinuxBuilder} -g ${toString cfg.configurationLimit} -t -1 -r -d /boot -c "$1"
          # A MENU line makes U-Boot always ask; the labels must fit the 30-column screen.
          ${pkgs.gnused}/bin/sed -i -E \
            -e '/^MENU TITLE/d' \
            -e 's/MENU LABEL NixOS - Configuration ([0-9]+)-default \(([^ ]+ [^ ]+) - .*\)$/MENU LABEL \1: \2/' \
            /boot/extlinux/extlinux.conf
        ''
        + lib.optionalString (cfg.device != null) ''
          img=$1/boot.img
          size=$(stat -L -c %s "$img")
          if ! cmp -s -n "$size" "$img" ${cfg.device}; then
            echo "writing boot.img to ${cfg.device}"
            dd if="$img" of=${cfg.device} bs=1M conv=fsync status=none
          fi
        ''
      );
    };

    system.systemBuilderCommands = ''
      ln -s ${bootImage} $out/boot.img
    '';
  };
}
