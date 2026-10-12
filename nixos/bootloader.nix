{
  config,
  lib,
  pkgs,
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
in
{
  options.boot.loader.agm-m7 = {
    enable = lib.mkEnableOption "writing boot.img to the boot partition";
    device = lib.mkOption {
      type = lib.types.str;
      default = "/dev/disk/by-partlabel/boot";
      description = "Partition that LK boots boot.img from.";
    };
  };

  config = lib.mkIf cfg.enable {
    boot.loader.grub.enable = false;
    boot.loader.initScript.enable = false;

    system.build = {
      inherit bootParts bootImage;
      installBootLoader = pkgs.writeShellScript "install-agm-m7-boot-img" ''
        set -euo pipefail
        export PATH=${lib.makeBinPath [ pkgs.coreutils pkgs.diffutils ]}
        img=$1/boot.img
        size=$(stat -L -c %s "$img")
        if cmp -s -n "$size" "$img" ${cfg.device}; then
          exit 0
        fi
        echo "writing boot.img to ${cfg.device}"
        dd if="$img" of=${cfg.device} bs=1M conv=fsync status=none
      '';
    };

    system.systemBuilderCommands = ''
      ln -s ${bootImage} $out/boot.img
    '';
  };
}
