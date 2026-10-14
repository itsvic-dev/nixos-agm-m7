{
  lib,
  pkgs,
  firmware,
  ...
}:

let
  files = [
    "ROMv2_lm_patch_1_0_hdr.bin"
    "ROMv2_lm_patch_1_1_hdr.bin"
    "WIFI_RAM_CODE_6755"
    "WMT_SOC.cfg"
  ];

  # No fsck and noload, so Linux never writes to the phone's own partitions.
  readOnly = label: {
    device = "/dev/disk/by-partlabel/${label}";
    fsType = "ext4";
    noCheck = true;
    options = [
      "ro"
      "noload"
      "nofail"
    ];
  };
in
{
  fileSystems."/nvdata" = readOnly "nvdata";
  fileSystems."/protect_f" = readOnly "protect1";
  fileSystems."/protect_s" = readOnly "protect2";

  hardware.firmwareCompression = "none";
  hardware.firmware = [
    (pkgs.runCommand "vendor-firmware" { } ''
      mkdir -p $out/lib/firmware
      for f in ${lib.escapeShellArgs files}; do
        cp ${firmware}/proprietary/vendor/firmware/$f $out/lib/firmware/$f
      done
      ln -s /nvdata/APCFG/APRDEB/WIFI $out/lib/firmware/mt6739-wlan-nvram.bin
      mkdir -p $out/lib/firmware/powervr
      cp ${firmware}/proprietary/vendor/firmware/rgx.fw.signed $out/lib/firmware/powervr/rogue_22.49.21.16_ddk19.fw
    '')
  ];
}
