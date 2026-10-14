# U-Boot research

Question: can mainline U-Boot run as the "kernel" of our `boot.img`, started by the stock LK, and give us fastboot over USB?

Short answer: yes, this is very likely possible. U-Boot already has almost all the parts. The main cost is a new SoC target, a small clock driver, and blind bring-up without UART.

Source: mainline U-Boot, shallow clone in `ref/u-boot` (commit `fed92546`, 2026-10-12).

Legend: **[src]** = verified in U-Boot or Linux source. **[inf]** = inference, not tested.

## 1. USB: MUSB gadget and fastboot

- **[src]** `drivers/usb/musb-new/mt85xx.c` (`CONFIG_USB_MUSB_MT85XX`) is a MediaTek MUSB glue with gadget support. Compatible: only `mediatek,mt8518-musb`. Users: `mt8516_pumpkin_defconfig` and `mt8518_ap1_emmc_defconfig`. `mt8516.dtsi` uses `"mediatek,mt8516-musb", "mediatek,mt8518-musb"`.
- **[src]** Both defconfigs enable `CONFIG_USB_FUNCTION_FASTBOOT`, `FASTBOOT_FLASH`, `FASTBOOT_FLASH_MMC_DEV=0` with `USB_MUSB_GADGET` + `USB_MUSB_MT85XX`. So fastboot over musb-new on MediaTek is an upstream configuration. Sunxi also enables fastboot on MUSB by default (`drivers/fastboot/Kconfig`).
- **[src]** The glue uses the legacy gadget path (no `DM_USB_GADGET`). The board probes the device in `board_late_init()` with `uclass_get_device(UCLASS_USB_GADGET_GENERIC, 0, ...)` (`board/mediatek/mt8516/mt8516_pumpkin.c`). Interrupts are polled through `dm_usb_gadget_handle_interrupts()` in `musb_uboot.c`, which calls the glue ISR. The ISR clears the MTK write-1-to-clear interrupt registers. PIO only, no DMA.
- **[src]** The glue programs the PHY with hard-coded byte writes at `USB20_PHY_BASE 0x11110800` (MT8516). The offsets (0x18 ACR6, 0x68 DTM0, 0x6c/0x6d DTM1) are the T-PHY v1 U2 COM bank layout. The writes to 0x6c/0x6d force IDDIG, VBUSVALID and AVALID, so the PHY is forced to device mode.
- **[src]** Our PHY is the same layout: Linux `u2port0` is at `0x11cc0800`, and T-PHY v1 puts the U2 COM bank at `port_base + 0x000`.
- **[src]** The glue needs three clocks named `usbpll`, `usbmcu`, `usb`. Our Linux DT uses `main`, `mcu`, `univpll`.
- **[src]** U-Boot has `drivers/phy/phy-mtk-tphy.c` with `mediatek,generic-tphy-v1`. It has `init`, `power_on`, `power_off`, but no `set_mode` (Linux has `u2_phy_instance_set_mode()` for IDDIG). The MUSB glue does not use the generic PHY API at all.

What we must add for MT6739:

1. Glue: add `mediatek,mt6739-musb` (or `mediatek,mtk-musb`). Take the PHY base from DT, not from a constant. Quick hack **[inf]**: change `USB20_PHY_BASE` to `0x11cc0800`; the existing byte sequence will very likely work. Clean way: use `generic_phy_get_by_index()` + T-PHY, and add `set_mode` (port 15 lines from Linux) to the U-Boot T-PHY driver.
2. Clocks: either a U-Boot clock driver for the needed gates and muxes, or `fixed-clock` stubs plus a few raw writes in board code (infra gate `CLK_INFRA_ICUSB` = INFRA CG0 bit 8, clear register `0x10001084`; `usb_top_sel` parent `univpll3_d4`). **[inf]** LK does not use USB in normal boot, so we cannot trust that it left the USB clock on.
3. The rest (musb core, gadget, f_fastboot, g_dnl) is generic.

## 2. MediaTek arm64 SoCs in U-Boot

- **[src]** `arch/arm/mach-mediatek`: MT7622, MT7981, MT7986, MT7987, MT7988, MT8183, MT8188, MT8189, MT8195, MT8365, MT8512, MT8516, MT8518 (arm64), MT7623, MT7629 (arm32). No phone SoC: no MT6735, MT6739, MT6761, MT6765 anywhere in `drivers/` or `arch/`.
- Closest by block:
  - USB: **MT8516/MT8518** (same MUSB + same U2 PHY layout). **[src]**
  - MSDC: **MT8183** compat data. Linux `mt6779_compat` equals `mt8183_compat` except `needs_top_base`. In U-Boot `top_base` is optional (`if (host->top_base)`). **[src]** U-Boot has no `mediatek,mt6779-mmc`, so we add one entry (copy `mt8183_compat`, maybe without `use_dma_mode` at first).
  - PMIC wrapper: **MT8365** (`drivers/power/pmic/mtk-pwrap.c` already knows the **MT6357** PMIC). We would add an MT6739 pwrap register table from our Linux `mtk-pmic-wrap.c`. Optional.
  - Clocks/pinctrl framework: `drivers/clk/mediatek/clk-mtk.c` and `drivers/pinctrl/mediatek/pinctrl-mtk-common.c` are generic. Our Linux `clk-mt6739.c` and `pinctrl-mt6739.c` can be ported to them.
- Minimum for a new SoC:
  - `TARGET_MT6739` in `arch/arm/mach-mediatek/Kconfig`, `mt6739/init.c` (`dram_init`, usable RAM top, `reset_cpu`), `board/mediatek/mt6739/`, `include/configs/mt6739.h`, a defconfig and a DTS. `MTK_MEM_MAP_DDR_SIZE` must cover 1 GiB (default is 512 MiB).
  - Timer: none. The arm64 generic timer works from `CNTFRQ_EL0`. **[src]** U-Boot writes `CNTFRQ` only at EL3, so it depends on ATF having set it (see risks).
  - GIC: none. U-Boot polls; it does not use interrupts.
  - Pinctrl: none at first **[inf]**. LK has already muxed the eMMC pins. Needed later for the SD card if LK does not set it up.
  - MSDC: one compat entry, a `source` clock with a correct rate (`clk_get_rate()` is used for the divider), and `hclk`.
  - Watchdog: `drivers/watchdog/mtk_wdt.c` (`mediatek,wdt`). **[src]** Its reset does not set `AUTO_START` or clear `DDR_RESERVE`; we must do that, as our Linux restart handler does. Do not copy `reset_cpu()` from MT8516: it uses PSCI `SYSTEM_RESET`, which hangs in our ATF.
  - Serial: not needed (no UART access).

## 3. Prior art

- I found no public mainline U-Boot that is chainloaded from MediaTek LK on a phone. Searches for MT6735/MT6765/MT6739 + U-Boot only found LK reverse engineering (Rabbit R1 boot notes, `lk-mt6765` forks) and the mt6735-mainline group (Linux only).
- **[src]** Very close prior art in U-Boot itself, same idea on other vendors:
  - `ARCH_SNAPDRAGON` (`doc/board/qualcomm/board.rst`): U-Boot as the kernel of an Android `boot.img`. Recipe: `gzip u-boot-nodtb.bin`, append the DTB, `mkbootimg`. Uses `OF_BOARD`, `SAVE_PREV_BL_FDT_ADDR`, `LINUX_KERNEL_IMAGE_HEADER`, fastboot.
  - `exynos-mobile` (`doc/board/samsung/exynos-mobile.rst`): Samsung phones, started by S-BOOT **or S-LK**, `u-boot-nodtb.bin` in `boot.img`, fastboot over USB, `POSITION_INDEPENDENT`.
  - `ARCH_MMP` (Marvell PXA1908 phones): selects `SAVE_PREV_BL_FDT_ADDR` and `SAVE_PREV_BL_INITRAMFS_START_ADDR`.

## 4. Build U-Boot as a kernel-like payload

- **[src]** `CONFIG_LINUX_KERNEL_IMAGE_HEADER` puts an arm64 `Image` header (`b reset`, text_offset, image_size, `ARM\x64` magic) at `_start` (`arch/arm/cpu/armv8/start.S`, `asm/boot0-linux-kernel-header.h`). It has no prompt; the target selects it. `LNX_KRNL_IMG_TEXT_OFFSET_BASE` sets text_offset (`TEXT_BASE` minus this base).
- **[src]** `CONFIG_POSITION_INDEPENDENT=y` (MT8516 Pumpkin, qcom, exynos-mobile use it). U-Boot can then run from wherever LK puts it (`0x40200000`) and relocate itself.
- DTB, **[src]**:
  - `SAVE_PREV_BL_FDT_ADDR` adds `save_boot_params()`, which keeps `x0` from the previous loader. `get_prev_bl_fdt_addr()` returns it, and the env gets `prevbl_fdt_addr`.
  - `OF_BOARD` + `board_fdt_blob_setup()` lets the board choose the control DT: the internal one, or the one in `x0`. Snapdragon and exynos-mobile both prefer the internal DT and fall back to `x0`.
- Proposed packing **[inf]**:
  - `boot.img` kernel = `gzip(u-boot.bin)` + `mt6739-agm-m7.dtb`.
  - `u-boot.bin` has U-Boot's own DT at its end (`OF_SEPARATE`).
  - The appended Linux DTB is only for LK. It must keep `-@` and the `lk-odm-overlay-sink` node, else LK asserts.
  - LK applies its overlay and fills `/memory`, `/chosen` (`atag,videolfb`, modem info), and reserved memory. Then it passes that DTB in `x0`. U-Boot reads RAM and framebuffer data from it.
  - Our `scripts/mkboot.py` needs no change. Only the "kernel" file changes.
- Booting Linux afterwards **[inf]**: U-Boot loads the Linux `Image`, DTB and initrd from another partition (or a FIT, or a v0 `boot.img` via `CONFIG_ANDROID_BOOT_IMAGE`). LK puts runtime data in its DTB, so U-Boot must copy it into the Linux DTB in `ft_board_setup()`: `/memory`, reserved memory (`mblock-*`, CCCI, CONSYS), `/chosen` `atag,videolfb*`, `ccci,modem_info_v2`, and the LK part of `bootargs`. Simpler first step: boot Linux with the `x0` DTB as is.
- Bonus: U-Boot `fastboot boot` takes a `boot.img` from RAM. That fixes our current problem where LK's `fastboot boot` uses the wrong DTB.

## 5. Effort and risks

Rough effort (evenings, with blind debugging), **[inf]**:

| Step | Effort |
|---|---|
| Target skeleton, defconfig, DTS, `dram_init`/RAM top from `x0` DTB, WDT reset | 1 to 2 days |
| Proof of life without UART (screen or RAM log, see below) | 0.5 to 1 day |
| DBI video driver for an on-screen console (port of the 300-line Linux `mt6739-dbi`, `video_sync` triggers `DBI_START`) | 1 day |
| Clocks: stubs + raw writes, or a subset port of `clk-mt6739.c` | 1 to 3 days |
| MSDC0 compat entry and clock rate | 0.5 day |
| MUSB glue for MT6739, PHY, fastboot to eMMC | 1 to 3 days |
| Boot Linux with DT fixups from the LK DTB | 1 to 2 days |
| Optional: pwrap + RTC reboot modes, pinctrl, SD card | 2 to 4 days |

Total: about 1 to 2 weeks of part-time work for "LK -> U-Boot -> fastboot flash -> boot Linux". Most of the risk is in the first step, because we cannot see anything.

Main risks:

1. **No UART.** Bring-up is blind. Options:
   - Draw to the LK framebuffer (`0x77ee0000`) and trigger `DBI_START`. The panel has its own frame memory, so nothing shows without the trigger.
   - Write a log to a fixed RAM area (for example a custom `DEBUG_UART` putc into the pstore region). Then reset with the watchdog, which keeps DRAM, boot Linux and read the log with `/dev/mem`.
   - USB CDC ACM console (`CONFIG_USB_FUNCTION_ACM` exists) once USB works.
2. **LK handoff details unknown.**
   - Exception level (EL1 or EL2).
   - Whether `CNTFRQ_EL0` is set. Our Linux DT has `clock-frequency = <13000000>` in the timer node, which hints that firmware may not set it. If it reads 0, U-Boot delays break and we must override the timer rate.
   - Whether LK checks anything in the Image header. **[inf]** It boots our `Image.gz` + DTB, so U-Boot with the same header should pass.
   - Check EL and CNTFRQ from Linux first (dmesg "CPU: All CPU(s) started at ELx", and a small `mrs cntfrq_el0` test).
3. **Memory map.** U-Boot relocates to the top of RAM. The top of RAM has the LK framebuffer (`0x77ee0000`), CCCI regions (`0x62000000`..`0x76bfffff` in normal boot), CONSYS memory and maybe ATF/TEE. We need `board_get_usable_ram_top()` below `0x62000000`. The fastboot buffer must also avoid these regions.
4. **Reset and power off.** PSCI reset and off hang in our ATF. U-Boot must use the watchdog with `AUTO_START` set and `DDR_RESERVE` cleared, else the bootloaders stall. Reboot to recovery or fastboot needs RTC `PDN1` bits through the pwrap, which is not in U-Boot for MT6739 yet.
5. **Clocks LK leaves off.** USB (and SD) may need gates and muxes that LK did not turn on in normal boot. MSDC needs the right source clock rate for the divider.
6. **Speed.** The MUSB glue is PIO only **[inf]**: a few MB/s. Good for `boot.img`, slow for a full root image.
7. **Brick risk is low but real.**
   - Test U-Boot from `recovery` first: LK boots it with `adb reboot recovery` or RTC bit 4, and `boot` keeps NixOS.
   - Stock LK fastboot stays available through RTC bit 13 / `reboot bootloader`.
   - mtkclient (akos's laptop) is the last fallback.
   - Do not let U-Boot overwrite the partition it runs from until fastboot is proven.

## 6. Boot menu design (lk2nd-like)

Goal: U-Boot in `boot`, started by LK in normal boot. It shows a menu with NixOS, LineageOS, LineageOS recovery and fastboot, and has its own fastboot.

### 6.1 What we get from LK's normal boot

- **[src]** LK loads and starts the modem only in normal boot (see `NOTES.md`). U-Boot runs in normal boot, so every OS that U-Boot starts gets the modem. Today LineageOS boots in recovery mode and has no modem. U-Boot fixes that.
- Both OSes use our 64-bit mainline kernel with the same `mt6739-agm-m7.dtb`. The LineageOS DTB only adds `/firmware/android` (`scripts/build-android-kernel.sh`). So U-Boot can pass the `x0` DTB (with LK's runtime data) to both kernels. For LineageOS it adds `/firmware/android` with `fdt` commands. **[inf]** No 32-bit kernel and no vendor DTB are involved; this removes the hardest problem.
- LK charger mode also boots `boot`, with `androidboot.mode=charger` in the `x0` `/chosen/bootargs`. U-Boot must see this and boot the default OS at once, without the menu.

### 6.2 Input

- **[src]** The phone has a D-pad on the keypad matrix: `KEY_UP` (4,3), `KEY_DOWN` (4,4), `KEY_ENTER` (6,3), plus `KEY_LEFT`, `KEY_RIGHT`, `KEY_BACK` and the digits (`mt6739-agm-m7.dts`). So `bootmenu` can use real arrow keys. No volume-key hacks are needed.
- **[src]** U-Boot has no MediaTek keypad driver. The keypad state is 5 read-only registers at `0x10010004` (`KPD_MEM1..5`, bits 15:0 of each, a bit is 0 when the key is down). The clock is `clk26m`.
- **[src]** `drivers/input/button_kbd.c` turns `UCLASS_BUTTON` devices with `linux,code` into a keyboard (stdin), and maps arrows to the escape codes that `bootmenu` reads.
- Plan **[inf]**: a small `button-mtk-keypad` driver (one child node per key, with row and column), or a `UCLASS_KEYBOARD` driver that polls `KPD_MEM`. About 100 lines either way. LK uses the keypad for its boot mode keys, so the keypad is very likely powered and configured.

### 6.3 Display

- The menu needs the DBI video driver from section 5 (port of `mt6739-dbi`). The panel is 240 px wide: with the 8x16 font that is 30 columns, enough for `bootmenu`.
- Before the video driver works, the menu cannot be seen. A safe default (autoboot NixOS after a few seconds) keeps the phone usable during bring-up.

### 6.4 Where each image lives

| Entry | Image | Place | How U-Boot starts it |
|---|---|---|---|
| U-Boot | `boot.img` (U-Boot + DTB for LK) | eMMC `boot` (24 MiB) | LK, normal boot |
| NixOS | kernel, initrd, DTB per generation | microSD, new FAT or ext4 `/boot` partition | `extlinux.conf` (`bootmeth_extlinux`), from `boot.loader.generic-extlinux-compatible` |
| LineageOS | `boot.img` (header v0) | see open question 1 | `mmc read` + `bootm` (`CONFIG_ANDROID_BOOT_IMAGE`) |
| LineageOS recovery | `recovery.img` | eMMC `recovery` | `bootm` from U-Boot, or set `RTC_PDN1` bit 4 and reset, so LK boots it |
| Fastboot | none | none | `fastboot usb 0` |

- **[src]** With extlinux, the menu can also list the NixOS generations. That fixes "LK has no menu, so older generations cannot be picked at boot" (`NOTES.md`). `nixos/bootloader.nix` would then stop writing `boot`.
- **[src]** NixOS from the microSD needs MSDC1 in U-Boot. MSDC1 needs `vmch` (3.3 V) and `vmc` from the PMIC. **[inf]** LK probably does not power the card in normal boot. Then U-Boot needs pwrap (section 2) and a few PMIC regulator writes. Fallback: a NixOS `boot.img` in an eMMC partition, which needs only MSDC0.
- If `recovery` keeps a real recovery image, LK's own recovery path ("reboot recovery", RTC bit 4) still works without U-Boot. So the recovery entry can be LK's.

### 6.5 Fastboot

- **[src]** U-Boot fastboot finds partitions by GPT name on `FASTBOOT_FLASH_MMC_DEV` (eMMC, `mmc 0`). Other devices use `dev:part`, for example `fastboot flash 1:super super.img` for the microSD (`do_get_part_info()` in `drivers/fastboot/fb_mmc.c`). Sparse images are supported (`IMAGE_SPARSE`).
- **[src]** `fastboot boot` works on a full `boot.img` in RAM, with the right DTB.
- Aliases (`fastboot_partition_alias_lineage=...`) give short names.
- **[inf]** PIO only: a 500 MiB sparse `super` takes minutes. For `super`, `scripts/android-flash.sh` over SSH from NixOS stays faster.
- Safety: LK fastboot stays (RTC `PDN1` bit 13, `reboot bootloader` from Linux). We do not yet know an LK key combination for fastboot or recovery at power on. Find it (LK disassembly, `mtk_detect_key`) before U-Boot replaces the NixOS `boot.img` in `boot`. Without it, a broken U-Boot that cannot start Linux needs mtkclient on akos's laptop.

### 6.6 Picking the OS from a running system

- `reboot recovery` keeps working (LK, RTC bit 4), it skips U-Boot.
- For "reboot into LineageOS with modem" or "boot NixOS once", U-Boot needs a flag that survives a reset. Options:
  1. U-Boot environment in a partition, written with `fw_setenv` from NixOS and from LineageOS (root). Simple and standard (`ENV_IS_IN_MMC`).
  2. A magic word in reserved DRAM (the watchdog reset keeps DRAM). No storage writes, but lost on power off.
  3. RTC `PDN1`/`PDN2` spare bits. Needs pwrap in U-Boot.
- Recommendation: option 1, `boot_once=lineage` cleared by U-Boot after it reads it.

### 6.7 Extra effort on top of section 5

| Step | Effort |
|---|---|
| Keypad driver for `button_kbd` | 0.5 day |
| `bootmenu` entries, charger-mode bypass, env in a partition, `fw_setenv` config | 1 day |
| Android `boot.img` boot with `/firmware/android` fixup and `androidboot.*` from LK | 1 day |
| MSDC1 + PMIC power for the microSD (if LK leaves it off) | 1 to 2 days |
| NixOS: extlinux on the microSD, new bootloader module | 1 day |

Total with section 5: about 2 to 3 weeks of part-time work.

### 6.8 Open questions

1. LineageOS `boot.img` place. Today it is in `recovery`. If `recovery` gets a real recovery image, `boot.img` needs a new place: `cache` (208 MiB), `system` (1.5 GiB) or `vendor` (504 MiB) on the eMMC are free if we do not need the old stock LineageOS GSI. Or a partition on the microSD.
2. Does the LineageOS 24.0 device tree build a `recovery.img`? Header v0 and no A/B mean a separate image is needed.
3. Is the old LineageOS GSI (eMMC `system`/`vendor`, FDE `userdata`) still needed?
4. Autoboot default and delay: NixOS after 3 s?

### 6.9 Alternatives

- Patch LK: it is a closed binary, so every feature is reverse engineering work. U-Boot already has fastboot, menus, extlinux and Android images.
- lk2nd: Qualcomm only. No MediaTek port exists.

## Sources

- U-Boot: `drivers/usb/musb-new/mt85xx.c`, `musb_uboot.c`, `drivers/usb/gadget/udc/udc-uclass.c`, `drivers/phy/phy-mtk-tphy.c`, `drivers/mmc/mtk-sd.c`, `drivers/watchdog/mtk_wdt.c`, `drivers/power/pmic/mtk-pwrap.c`, `arch/arm/mach-mediatek/`, `configs/mt8516_pumpkin_defconfig`, `configs/mt8518_ap1_emmc_defconfig`, `arch/arm/lib/save_prev_bl_data.c`, `arch/arm/mach-snapdragon/board.c`, `doc/board/qualcomm/board.rst`, `doc/board/samsung/exynos-mobile*.rst`.
- U-Boot boot menu: `drivers/input/button_kbd.c`, `drivers/fastboot/fb_mmc.c`, `doc/usage/cmd/bootmenu.rst`, `doc/android/fastboot.rst`, `boot/bootmeth_extlinux.c`, `boot/image-android.c`.
- Our Linux tree: `arch/arm64/boot/dts/mediatek/mt6739.dtsi`, `drivers/usb/musb/mediatek.c`, `drivers/phy/mediatek/phy-mtk-tphy.c`, `drivers/mmc/host/mtk-sd.c`, `drivers/clk/mediatek/clk-mt6739.c`, `drivers/gpu/drm/tiny/mt6739-dbi.c`.
- Web: https://github.com/DavidBuchanan314/rabbit_r1_boot_notes, https://github.com/DavidBuchanan314/lk-mt6765, https://gitlab.com/mt6735-mainline, https://postmarketos.org/blog/2018/04/14/lowlevel/ (no U-Boot over MediaTek LK found).
