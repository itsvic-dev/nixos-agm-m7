{ pkgs, ... }:

let
  files = [
    "ROMv2_lm_patch_1_0_hdr.bin"
    "ROMv2_lm_patch_1_1_hdr.bin"
    "WIFI_RAM_CODE_6755"
    "WMT_SOC.cfg"
  ];
in
{
  # The blobs stay on the phone's own vendor partition. No fsck and noload, so Linux never writes to it.
  fileSystems."/vendor" = {
    device = "/dev/disk/by-partlabel/vendor";
    fsType = "ext4";
    noCheck = true;
    options = [
      "ro"
      "noload"
      "nofail"
    ];
  };

  hardware.firmwareCompression = "none";
  hardware.firmware = [
    (pkgs.runCommand "vendor-firmware" { } ''
      mkdir -p $out/lib/firmware
      for f in ${toString files}; do
        ln -s /vendor/firmware/$f $out/lib/firmware/$f
      done
    '')
  ];
}
