{ pkgs, kernel, ... }:

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

  boot.loader.grub.enable = false;
  boot.loader.initScript.enable = false;
}
