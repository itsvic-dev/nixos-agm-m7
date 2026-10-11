# AGM M7 notes

## Hardware

- SoC: MediaTek MT6739 (4x Cortex-A53, GICv3, PowerVR GE8100).
- RAM: 1 GiB at 0x40000000. eMMC: 7.28 GiB (FN62BB).
- PMIC: MT6357.
- Panel: GalaxyCore GC9306 (`gc9306_dbi_c_qvgal`), 240x320, DBI command mode.
- Stock firmware: Android 8.1, 32-bit kernel 4.4.95 (`bootopt=64S3,32S1,32S1`).

## Bootloader (LK)

LK loads at 0x46000000. Facts found by disassembly:

- `boot.img` is header v0. The kernel blob is `Image.gz` (or `zImage`) with the DTB appended.
- `bootopt=64S3,32N2,64N2` in the header cmdline selects a 64-bit kernel. The kernel address must be 512 KiB aligned.
- LK loads the DTB from the `boot` or `recovery` partition early, before fastboot. So `fastboot boot` runs our kernel with the DTB of the `boot` partition. Do not use it.
- LK applies the ODM overlay from `odmdtbo` to our DTB, and asserts (silent hang before the logo) if that fails. Our DTB is built with `-@` and has the node `lk-odm-overlay-sink` with all 41 labels that the overlay needs. Do not reuse those labels (for example `pio`) on real nodes.
- `fastboot getvar` hangs LK. `fastboot reboot recovery` boots normally.
- LK passes the framebuffer in `/chosen` as `atag,videolfb-*`.

## Test loop

1. Boot LineageOS (GSI). Root is `phh-su -c`.
2. `scripts/build.sh` in the `agm-m7` OrbStack VM.
3. `scripts/test-recovery.sh <name>` writes `out/boot.img` to `recovery`, runs `adb reboot recovery`, waits for LineageOS, and copies pstore to `logs/<name>/`.

`fastboot flash recovery out/boot.img` also works.

Faster, from a running NixOS: `scripts/nixos-flash.sh` copies `out/boot.img` over SSH, writes it to `recovery` (`mmcblk0p2`), and runs `systemctl reboot --reboot-argument=recovery`. About 50 s per round trip.

- `reboot recovery`: the RTC driver sets `RTC_PDN1` bit 4 and triggers `WRTGR`, as the vendor kernel does. LK boots `recovery` once and clears the bit. `reboot bootloader` sets bit 13 (fastboot).
- The USB gadget needs fixed MAC addresses (`g_cdc.dev_addr`, `g_cdc.host_addr`). With random ones, macOS often does not create the network interface after a reboot.

## Debug without UART

- `earlycon=pstoreram,0x4445f000` writes the kernel log into the stock pstore console zone. LineageOS shows it as `/sys/fs/pstore/console-ramoops`. RAM survives normal resets and long-press resets. A battery pull clears it.
- `mtk_wdt_early=0x10007000,30` arms the watchdog early. A hung kernel resets after 30 s.
- USB: `g_cdc` gives a serial shell (`/dev/cu.usbmodem*`) and ECM networking. The phone is 172.16.42.1. Set the Mac to 172.16.42.2/24. `scripts/tnet.sh` runs commands over telnet.
- The initramfs reboots after 300 s unless `/tmp/stay` exists.

## Restart

PSCI `SYSTEM_RESET` and `SYSTEM_OFF` both hang in the ATF. The watchdog is stopped during shutdown, so a hung power off needs a battery pull (a long press did not help). We power off through the PMIC instead: clear `RG_PWRHOLD` (`0xa08` bit 0), as the vendor kernel does. With USB connected, the phone then starts offline charging. The `mtk_wdt` restart handler must run first (`mediatek,lk-normal-restart`, priority 192). It sets `AUTO_START` (the bootloaders read it as "boot without power key") and clears `DDR_RESERVE` (else the bootloaders stall on the orange-state screen).

## userdata

`userdata` (`mmcblk0p33`) has full-disk encryption: dm-crypt `aes-cbc-essiv:sha256` with a 128-bit key, f2fs inside.

- `tools/dmtable.c` (static armhf, run as root on LineageOS) prints the dm table with the key. Keep it in `secrets/userdata-dm-table.txt`. Never commit it.
- `scripts/build.sh` puts the table into the initramfs. `dmctl create userdata rw "$(cat /etc/userdata.table)"` creates `/dev/dm-0`.
- Android runs `fsck.f2fs` on the raw encrypted partition at every boot, and it always fails with "Magic Mismatch". That is normal.

## postmarketOS

- Built with pmbootstrap in the VM: device `qemu-aarch64`, kernel `lts`, `service_manager = openrc`, UI `console`. The root image goes to `/data/pmos.img` (2 GiB ext4).
- The initramfs mounts it with `losetup` and `mount -t ext4` (busybox `mount -o loop` fails), moves `/data` to `/mnt/data`, and runs `switch_root`.
- The initramfs boots `/data/nixos.img` first, then `/data/pmos.img`. `/data/<name>.disable`, or `touch /tmp/noboot` within 10 s, skips an image.
- `ssh user@172.16.42.1`.
- Do not upstream: pmOS (Nura) and Mobile NixOS do not accept AI-written code.

## NixOS

- `flake.nix` builds NixOS 26.05 for aarch64 on the Rosetta builder. The kernel and the initramfs still come from `boot.img` (`boot.kernel.enable = false`, `boot.initrd.enable = false`).
- `scripts/nixos-push.sh` builds the root image, grows it to 3 GB on the Mac (Android's `truncate` and `resize2fs` cannot), and copies it to `/data/nixos.img`.
- `scripts/nixos-deploy.sh [boot|switch]` copies a new system closure over SSH and sets the system profile. `/sbin/init` in the image points to that profile, so `boot` takes effect at the next boot from `recovery`.
- `ssh root@172.16.42.1`. `ttyGS0` has a root autologin. systemd-networkd gives the Mac 172.16.42.2 over DHCP.
- The kernel needs `NETDEVICES` (else `SO_BINDTODEVICE` fails and networkd, resolved and sshd break) and nftables. `MEMCG` crashes the kernel when systemd loads BPF programs, so it stays off.
- At shutdown, systemd cannot unmount `/mnt/data`, because the root image is on it.

## PMIC

- The pwrap registers are the same as on MT6765, but the preloader also enables the INT1 interrupt group. The mainline MT6765 entry ignores INT1, and the pwrap interrupt then storms until the watchdog fires. The MT6739 entry turns INT1 off.
- The PMIC interrupt is EINT 182 (virtual pin 168), level high.
- The pwrap reset is infracfg `0x140` bit 0.
- Long-press reset: the mainline keys driver turns it off unless the DT sets it. We keep the stock setting: one key, `power-off-time-sec = <0>`.
- The mainline auxadc scale did not divide by the ADC resolution. Fixed. `bat_adc` gives the battery voltage (`adc-battery`, matches the stock kernel within about 60 mV while charging). `batt_v` reads garbage, and some channels time out.
- Battery percent: `ocv-capacity-table-0` comes from the stock `battery0_profile_t1` (25 °C) in `bat_gm30`. Each profile entry is (depth, OCV in 0.1 mV, resistance).
- Charger detection: `RGS_CHRDET` is `CHR_TOP_CON0` (0xa88) bit 4. The `CHRDET` interrupt fires on plug and unplug. Do not use the label `charger`: the LK overlay sink has it.
- Charging: an external HL7005 (bq24157 family) at I2C1 0x6a charges the battery. LineageOS binds driver `hl7005` to `1-006a` and leaves the MT6357 charger off (`CHR_EN` = 0, `CHRWDT_OUT` = 1). Programming the MT6357 charger did nothing useful: the charge current we saw came from the HL7005, still set up by LineageOS. LineageOS has `/sys/devices/platform/mt-pmic/pmic_access` to read PMIC registers (write the address, then read).
- Gauge current: `FGADC_CUR_CON0` (0xd8a), latched with `FGADC_CON1` (`SW_READ_PRE`, wait for bit 15, read, `SW_CLEAR`). s16, 305.176 uA per LSB with 10 mOhm, times `CAR_TUNE` 0.93. Positive is charging.
- Battery NTC: 10 kOhm with 16.9 kOhm pull-up to 1.8 V (vendor default), on auxadc `batt_temp`. Our value matched LineageOS within 0.5 C.
- auxadc fixes for MT6357: `ISENSE` and `VCDT` used request bit 0 (should be 1 and 2), and `ISENSE` is 15 bits, not 12.
- The stock kernel also reads a wrong date from the RTC. Android keeps the time in software.

## Keypad

The keypad is an 8x9 matrix (key = row * 9 + col). The hang-up key is the PMIC power key. The PMIC home key is not connected.

| Row, col | Key |
|---|---|
| 0,1 | side key (`KEY_F11`) |
| 3,3 / 5,3 | left / right soft key (`KEY_MENU` / `KEY_BACK`) |
| 4,3 / 5,4 / 4,4 / 3,4 / 6,3 | D-pad up / right / down / left / center (`KEY_ENTER`) |
| 7,3 | call (`KEY_SEND`) |
| 7,4 3,5 6,4 4,5 5,5 6,5 7,5 3,6 4,6 | 1 to 9 |
| 5,6 / 6,6 / 7,6 | * / 0 / # |

## Display

LK turns on the panel and the path OVL0 -> RDMA0 -> DBI0 (mutex 0). The panel has its own frame memory, so each frame needs a trigger. The `mt6739-dbi` DRM driver keeps LK's setup and, for each update:

1. waits until DBI `STA` (0x14013000) bit 4 (busy) is 0,
2. sets OVL layer 3: `L3_CON` (0x090) format XRGB8888 with alpha off, `L3_PITCH` (0x0a4), `L3_ADDR` (0xfa0), and `SRC_CON` (0x02c) to layer 3 only,
3. writes `0x2c` to `SCMD0` (0xf80), pulses `START` (0x00c) bit 15, and sets mutex 0 `EN` (0x14001020).

- LK uses layer 0 for the logo and layer 3 (with per-pixel alpha) for the text. XRGB buffers have alpha 0, so alpha must be off.
- The mode comes from DBI `ROI_SIZE` (0x06c), which LK sets to 240x320.
- LK's framebuffer is at 0x77ee0000 (`atag,videolfb`, `mblock-4-framebuffer`, no-map). `/dev/mem` reaches it only through mmap (`devmem`), not `dd`.
- The DBI register layout is the same as the vendor `mt6771/dispsys/ddp_reg_dbi.h`.

## Hardware status

| Part | Status |
|---|---|
| CPUs, GIC, timer, PSCI | works |
| Clocks (apmixedsys, topckgen, infracfg) | works, `clk_ignore_unused` on cmdline |
| Pinctrl, GPIO, EINT | works |
| eMMC | works, 52 MHz, `vemc` from the PMIC |
| USB device | works, PIO only |
| Watchdog, restart | works |
| Display | works (`mt6739-dbi` DRM, fbcon), relies on LK's setup |
| Keypad, power key | works (`mt6779-keypad`, `mtk-pmic-keys`) |
| PMIC (pwrap, MT6357 regulators, RTC, auxadc) | works, `regulator_ignore_unused` on cmdline |
| Power off | works (PMIC `RG_PWRHOLD`) |
| Battery voltage, percent | works (`adc-battery` with the stock OCV table) |
| Charger detection, fuel gauge | works (`mt6357-charger`) |
| Charging | not done (external HL7005 on I2C1) |
| Userspace | NixOS 26.05 from `/data/nixos.img` |
| SD card, Wi-Fi, BT, modem, audio, camera, GPU | not done |
