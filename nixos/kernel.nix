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
    # Overrides LK's slub_debug=OFZPU; its red zones crash MEMCG.
    "slub_debug=-"
  ];

  boot.consoleLogLevel = 8;

  # With the default 3.3 MiB reserve, a full page cache starves atomic allocations in the USB network.
  boot.kernel.sysctl."vm.min_free_kbytes" = 16384;

  boot.loader.agm-m7 = {
    enable = true;
    # U-Boot is in the boot partition and boots NixOS with extlinux.
    device = null;
  };
}
