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
| eMMC | Y | HS200 (192 MHz). `userdata` is encrypted (dm-crypt), see below |
| USB device | Y | Serial shell and ECM networking. PIO only, no DMA |
| Watchdog, restart | Y | Through `mtk_wdt`. PSCI reset and power off hang in the ATF |
| Power off | Y | Through the PMIC. With USB connected, the phone starts offline charging |
| PMIC (MT6357): regulators, RTC, auxadc | Partial | `regulator_ignore_unused` on the cmdline. RTC reads a wrong year |
| Keypad, power key | Y | All 22 keys. Hold the hang-up key for a reset |
| Display | Partial | DRM and fbcon work. The driver reuses the panel setup from the bootloader. No backlight control |
| Battery | Partial | `mt6357-charger` fuel gauge: voltage, current, temperature, coulomb counting from an OCV start value. Status comes from the gauge current |
| Charger | Partial | External HL7005 on I2C1 (`hl7005`): 4.20 V, input limit 500 mA, watchdog, 0 to 45 °C limit. No USB charger type detection, so always 500 mA input |
| Touchscreen | Y | Telink TLSC6x at I2C0 0x38, works with `edt-ft5x06` (FT6x36 layout) |
| I2C | Y | I2C0 (touch), I2C1 (charger). Other buses not added yet |
| SD card | N | |
| Wi-Fi | Partial | Vendor gen2 driver in staging, iwd. Starts at boot. Firmware and NVRAM from the phone's own partitions |
| Bluetooth, GPS, FM | N | MT6739 connectivity (CONSYS) |
| Modem | N | |
| Audio | N | |
| Camera, flashlight | N | |
| Vibrator | N | |
| GPU (PowerVR GE8100) | Partial | Vendor DDK 1.9 firmware on the mainline `powervr` driver, see [docs/GPU.md](docs/GPU.md). Mesa Vulkan 1.2 (vkcube), and GL 2.1 / GLES 2.0 through Zink (WebGL 1 in Firefox). Not loaded at boot (`modprobe powervr`). Sway still renders with pixman |

## How it boots

The bootloader (LK) cannot boot an image from USB here, so we use the `recovery` partition:

- A normal boot starts the stock system (LineageOS) from `boot`.
- A boot into recovery (`adb reboot recovery` from LineageOS, or `systemctl reboot --reboot-argument=recovery` from NixOS) starts our kernel from `recovery`.
- The NixOS systemd initrd unlocks `userdata`, mounts it at `/run/userdata`, loop-mounts `nixos.img` as root, and starts the current system profile (`init=/nix/var/nix/profiles/system/init`).

LineageOS stays usable. We use it for the first install, and to read the kernel log from pstore after a crash.

## Requirements

- An AGM M7 with an unlocked bootloader, running a LineageOS GSI with root (`phh-su`).
- `adb` on the host.
- Nix with flakes and an `aarch64-linux` builder. The flake builds the kernel (from [linux-mt6739-agm-m7](https://github.com/itsvic-dev/linux-mt6739-agm-m7), branch `agm-m7`), the initrd and the NixOS system.
- Optional, for fast kernel development: a Linux arm64 machine with a checkout in `linux/`. We use an OrbStack VM named `agm-m7` (Debian, with `gcc`, `make`, `bc`, `flex`, `bison`, `libssl-dev`, `cpio`, `python3` and a static `busybox` at `/bin/busybox`).

## Build and install

1. Dump the `userdata` key once. Build `tools/dmtable.c` as a static armhf binary, run it as root on LineageOS, and save its output as `secrets/userdata-dm-table.txt`. Never commit this file.

2. Build the NixOS image and copy it to the phone (with LineageOS running):

   ```
   ./scripts/nixos-push.sh
   ```

   This writes `/data/nixos.img` (4 GB).

3. Build `boot.img`:

   ```
   ./scripts/mkboot-nix.sh
   ```

   Nix builds the kernel, the DTB and the initrd. The script appends the `userdata` key as a second cpio archive, so the key never enters the Nix store, and writes `out/boot.img`.

4. Flash it to `recovery` and boot it, from LineageOS:

   ```
   ./scripts/test-recovery.sh <log name>
   ```

   The script waits for LineageOS to come back, then copies the kernel log from pstore to `logs/<log name>/`. Stop it with Ctrl-C if you want to stay in NixOS.

## Update

From a running NixOS, LineageOS is not needed:

- NixOS only: `./scripts/nixos-deploy.sh switch` (now) or `boot` (at the next boot).
- Kernel or initrd: commit and push the kernel, `nix flake update linux`, `./scripts/nixos-deploy.sh boot`, `./scripts/mkboot-nix.sh`, then `./scripts/nixos-flash.sh`. It writes `out/boot.img` to `recovery` and reboots into it (about 50 s).
- Fast kernel development: `./scripts/config.sh` and `./scripts/build.sh` in the VM build `Image.gz` and the DTB incrementally from `linux/`. `KERNEL=vm ./scripts/mkboot-nix.sh` then packs them with the NixOS initrd, and `./scripts/nixos-flash.sh` flashes the result.

## Use

- SSH: `ssh root@172.16.42.1`. The phone gives the host 172.16.42.2 over DHCP.
- Serial: `/dev/cu.usbmodem*` (macOS) or `/dev/ttyACM*` (Linux), with root autologin.
- The screen runs `labwc` (wlroots, pixman renderer) as `user`, with the `foot` terminal and the `wvkbd` on-screen keyboard. The keypad and the touchscreen both work.
- `systemctl reboot` goes back to LineageOS. `systemctl reboot --reboot-argument=recovery` boots NixOS again.
- If the initrd fails, it writes the failed units to the kernel log and reboots into LineageOS. `test-recovery.sh` or `cat /sys/fs/pstore/console-ramoops` then shows the log.

## Layout

- `linux/`: kernel tree (separate repo). Board DT: `arch/arm64/boot/dts/mediatek/mt6739-agm-m7.dts`. Config fragment: `arch/arm64/configs/agm-m7.config`.
- `flake.nix`, `nixos/`: kernel, initrd, `boot.img` parts, NixOS configuration and root image.
- `scripts/`: build, flash, deploy and test helpers. `scripts/re/` has LK and boot.img analysis helpers.
- `tools/`: small device-mapper helpers.
- `stock/`: decompiled stock DT and ODM DTBO.

## Planned

- Touchscreen, Weston, backlight. See `docs/ROADMAP.md`.
