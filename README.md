# AGM M7 mainline

Mainline Linux and NixOS on the AGM M7, a rugged keypad phone with a MediaTek MT6739 SoC.

- The kernel tree is in `linux/` (branch `agm-m7`, a separate Git repo, based on Linux 7.2.9).
- The NixOS system is in `flake.nix` and `nixos/`.
- `docs/NOTES.md` has the hardware findings. `docs/ROADMAP.md` has the plan and the open items.

## Hardware status

| Part | Status | Notes |
|---|---|---|
| CPU (4x A53), GIC, timer, PSCI | Y | |
| Clocks, pinctrl, GPIO, EINT | Y | `clk_ignore_unused` on the cmdline |
| eMMC | Y | 52 MHz. `userdata` is encrypted (dm-crypt), see below |
| USB device | Y | Serial shell and ECM networking. PIO only, no DMA |
| Watchdog, restart | Y | Through `mtk_wdt`. PSCI reset and power off hang in the ATF |
| Power off | Y | Through the PMIC. With USB connected, the phone starts offline charging |
| PMIC (MT6357): regulators, RTC, auxadc | Partial | `regulator_ignore_unused` on the cmdline. RTC reads a wrong year |
| Keypad, power key | Y | All 22 keys. Long-press reset does not work |
| Display | Partial | DRM and fbcon work. The driver reuses the panel setup from the bootloader. No backlight control |
| Battery | Partial | `mt6357-charger` fuel gauge: voltage, current, temperature, coulomb counting from an OCV start value. Status comes from the gauge current |
| Charger | Partial | USB detection works. The charger is an external HL7005 on I2C1 (not the MT6357 one), and it has no driver yet, so the battery does not charge under Linux. Charge in LineageOS or when powered off |
| Touchscreen | Y | Telink TLSC6x at I2C0 0x38, works with `edt-ft5x06` (FT6x36 layout) |
| I2C | Y | I2C0 (touch), I2C1 (charger). Other buses not added yet |
| SD card | N | |
| Wi-Fi, Bluetooth, GPS, FM | N | MT6739 connectivity (CONSYS) |
| Modem | N | |
| Audio | N | |
| Camera, flashlight | N | |
| Vibrator | N | |
| GPU (PowerVR GE8100) | N | Software rendering only |

## How it boots

The bootloader (LK) cannot boot an image from USB here, so we use the `recovery` partition:

- A normal boot starts the stock system (LineageOS) from `boot`.
- A boot into recovery (`adb reboot recovery`) starts our kernel from `recovery`.
- Our initramfs unlocks `userdata`, mounts `/data/nixos.img` as a loop device, and starts NixOS from it.

LineageOS stays usable. We use it to flash `recovery` and to read the kernel log from pstore after a crash.

## Requirements

- An AGM M7 with an unlocked bootloader, running a LineageOS GSI with root (`phh-su`).
- `adb` on the host.
- A Linux arm64 build machine for the kernel. We use an OrbStack VM named `agm-m7` (Debian, with `gcc`, `make`, `bc`, `flex`, `bison`, `libssl-dev`, `cpio`, `python3` and a static `busybox` at `/bin/busybox`).
- Nix with flakes and an `aarch64-linux` builder for NixOS.

## Build and install

1. Dump the `userdata` key once. Build `tools/dmtable.c` as a static armhf binary, run it as root on LineageOS, and save its output as `secrets/userdata-dm-table.txt`. Never commit this file.

2. Build the kernel and `boot.img` in the build VM:

   ```
   ./scripts/config.sh
   ./scripts/build.sh
   ```

   This writes `out/boot.img`. The initramfs contains the `userdata` key.

3. Build the NixOS image and copy it to the phone (with LineageOS running):

   ```
   ./scripts/nixos-push.sh
   ```

   This writes `/data/nixos.img` (3 GB).

4. Flash `out/boot.img` to `recovery` and boot it:

   ```
   ./scripts/test-recovery.sh <log name>
   ```

   The script waits for LineageOS to come back, then copies the kernel log from pstore to `logs/<log name>/`. Stop it with Ctrl-C if you want to stay in NixOS. To flash by hand, `dd` the image to `/dev/block/platform/bootdevice/by-name/recovery` as root, then run `adb reboot recovery`.

## Use

- SSH: `ssh root@172.16.42.1`. The phone gives the host 172.16.42.2 over DHCP. If macOS does not create the USB network interface, replug the cable.
- Serial: `/dev/cu.usbmodem*` (macOS) or `/dev/ttyACM*` (Linux), with root autologin.
- The screen shows the console on tty1, with root autologin. The keypad works as a keyboard.
- To test a new kernel from a running NixOS: `./scripts/nixos-flash.sh`. It writes `out/boot.img` to `recovery` and reboots into it (about 50 s). LineageOS is not needed for this.
- To update NixOS without a new image: `./scripts/nixos-deploy.sh switch` (now) or `boot` (at the next boot).
- `systemctl reboot` goes back to LineageOS. `systemctl reboot --reboot-argument=recovery` boots NixOS again.
- In the initramfs, `touch /tmp/noboot` within 10 s keeps the busybox shell. `/data/nixos.disable` skips NixOS.

## Layout

- `linux/`: kernel tree (separate repo). Board DT: `arch/arm64/boot/dts/mediatek/mt6739-agm-m7.dts`. Config fragment: `arch/arm64/configs/agm-m7.config`.
- `nixos/`: NixOS configuration and root image.
- `initramfs/init`: busybox init that unlocks `userdata` and starts NixOS.
- `scripts/`: build, flash, deploy and test helpers. `scripts/re/` has LK and boot.img analysis helpers.
- `tools/`: small device-mapper helpers.
- `stock/`: decompiled stock DT and ODM DTBO.

## Planned

- Build the kernel and `boot.img` in the flake, so that one `nix build` gives everything.
- Touchscreen, Weston, backlight. See `docs/ROADMAP.md`.
