# Wi-Fi research

Status: research only, nothing ported yet (2026-10-11).

## What the stock system uses

- MT6739 integrated connectivity (CONSYS): Wi-Fi, BT, GPS, FM. No external combo chip.
- LineageOS loads `wmt_drv.ko` (WMT "conn_soc" stack), `wmt_chrdev_wifi.ko`, `wlan_drv_gen2.ko`, `bt_drv.ko`, `gps_drv.ko` and `fmradio_drv.ko` from `/vendor/lib/modules`. `ro.wlan.gen=gen2`, `mediatek.wlan.chip=CONSYS_MT6739`, chip ID 0x6739.
- Userspace: `wmt_loader` and `wmt_launcher` power the block on through `/dev/wmtdetect` and `/dev/stpwmt`.
- Firmware: `ROMv2_lm_patch_1_0_hdr.bin`, `ROMv2_lm_patch_1_1_hdr.bin`, `WIFI_RAM_CODE_6755` (the MT6755 name is used on MT6739), `WMT_SOC.cfg` (`co_clock_flag=1`, `coex_wmt_ant_mode=1`), `wifi_fw.cfg`.
- Copies of the firmware, the modules and the stock kernel log are in `logs/wifi/` (not in git).

## Hardware resources (stock DT)

- `consys@18070000` (`mediatek,mt6739-consys`): CONSYS registers, plus TOPRGU (0x10007000), TOPCKGEN (0x10000000) and SPM (0x10006000). IRQ SPI 233 (BGF, level) and SPI 235 (edge). Power domain `scpsys 3` (CONN).
- `wifi@180f0000`: Wi-Fi HIF DMA registers, IRQ SPI 234, 36-bit DMA, APDMA clock.
- `btif@1100c000` with APDMA channels 0x11000600/0x11000680: WMT talks to the CONSYS MCU over BTIF with the STP protocol. Wi-Fi needs BTIF too.
- `consys-reserve-memory`: 2 MiB, no-map (0x77c00000 on stock), shared with the CONSYS firmware, with EMI MPU protection and an EMI remap register.
- PMIC LDOs: `vcn18`, `vcn28`, `vcn33_wifi`, `vcn33_bt`. DCXO clock buffer for the connectivity clock.
- GPIO 118: GPS LNA. No Wi-Fi GPIOs.

## Sources

- The vendor tree in `ref/` has only the connectivity adapter layer. The WMT and Wi-Fi code was in the separate `vendor/mediatek/kernel_modules/connectivity` repo, which we do not have for MT6739.
- The stock modules are unstripped (1127 symbols in `wmt_drv.ko`), including the MT6739 platform functions: `mtk_wcn_consys_hw_pwr_on`, `consys_hw_power_ctrl`, `consys_clock_buffer_ctrl`, `consys_emi_set_remapping_reg`, `consys_hw_vcn18_ctrl` and others. Build path: `pq1181_agm_e55_eubig`.
- Closest public port: frank-w's BPI-R2 (MT7623N with MT6625L), the same conn_soc + gen2 + BTIF architecture, newest branch `6.0-mt6625` (2022): https://github.com/frank-w/BPI-Router-Linux/tree/6.0-mt6625. 32-bit, no MT6739 platform code, reported as unstable.
- Mainline has no driver for MT67xx CONNSYS. postmarketOS lists Wi-Fi as broken on MT6735 and MT6765.

## Options

1. USB host and a Wi-Fi dongle. Mainline MUSB (`drivers/usb/musb/mediatek.c`) supports host mode with a USB role switch. The board has a WUSB3801 Type-C controller (I2C3 0x60, EINT 112, mainline `wusb3801`) and an `otg_iddig` EINT (93). VBUS needs a boost regulator in our HL7005 driver (`OPA_MODE`). Estimate: a few evenings.
2. Integrated Wi-Fi. Rebase frank-w's port to 7.2 for arm64, write the MT6739 platform layer by reverse engineering `wmt_drv.ko`, port BTIF and the gen2 AHB HIF, replace `wmt_launcher`. Estimate: one to three months of part-time work, with a real chance of failure.

## Risks for option 2

- No MT6739 platform source; the power-on sequence must come from disassembly.
- The preloader or ATF may lock the EMI MPU region for CONSYS.
- No UART, so CONSYS firmware crashes are hard to debug.
- About 200k lines of gen2 and WMT code to keep building on a current kernel.
