{ pkgs, ... }:

let
  ccci-fsd = pkgs.callPackage ../tools/ccci-fsd/package.nix { };
  data = "/var/lib/ccci-fs/data";

  # The modem writes its NVRAM at runtime; it works on a copy, never on the phone's partitions.
  seed = pkgs.writeShellScript "ccci-fs-seed" ''
    set -eu
    [ -d ${data}/nvdata/md/NVRAM ] && exit 0
    mkdir -p ${data}/nvdata ${data}/protect_f ${data}/protect_s
    cp -a /nvdata/md /nvdata/md_cmn ${data}/nvdata/
    cp -a /protect_f/md ${data}/protect_f/
    cp -a /protect_s/md ${data}/protect_s/
  '';

  start = pkgs.writeShellScript "modem-start" ''
    for f in /sys/bus/platform/drivers/mt6739-ccci/*/state; do
      echo 1 > "$f"
    done
  '';
in
{
  systemd.services.ccci-fsd = {
    description = "MT6739 modem NVRAM file service and modem start";
    wantedBy = [ "multi-user.target" ];
    wants = [
      "nvdata.mount"
      "protect_f.mount"
      "protect_s.mount"
    ];
    after = [
      "nvdata.mount"
      "protect_f.mount"
      "protect_s.mount"
    ];
    serviceConfig = {
      StateDirectory = "ccci-fs";
      StateDirectoryMode = "0700";
      ExecStartPre = seed;
      ExecStart = "${ccci-fsd}/bin/ccci-fsd /dev/ccci_fs ${data}";
      ExecStartPost = start;
      Restart = "on-failure";
    };
  };
}
