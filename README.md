# AGM M7 mainline

Mainline Linux and NixOS on the AGM M7, a rugged keypad phone with a MediaTek MT6739 SoC.

- Kernel: [linux-mt6739-agm-m7](https://github.com/itsvic-dev/linux-mt6739-agm-m7), branch `agm-m7`, based on Linux 7.2.9.
- NixOS system: `flake.nix` and `nixos/`.
- Hardware findings: [docs/NOTES.md](docs/NOTES.md). Plan and open items: [docs/ROADMAP.md](docs/ROADMAP.md).

## Hardware status

| Part | Status | Notes |
|---|---|---|
| CPU, GIC, timer, PSCI | Yes | cpuidle and cpufreq |
| Clocks, pinctrl, GPIO | Yes | |
| eMMC | Yes | HS200 |
| microSD | Yes | UHS-I SDR104, NixOS root |
| USB | Partial | Device mode only |
| Display | Partial | Uses the bootloader's panel setup |
| Touchscreen | Yes | |
| Keypad, power key | Yes | All 22 keys |
| GPU (PowerVR GE8100) | Partial | Vulkan 1.2, GLES 2.0 through Zink |
| Audio | Yes | Speaker, earpiece, main mic |
| Wi-Fi | Yes | Vendor driver in staging |
| Modem | Partial | LTE data, no calls or SMS |
| Battery, charging | Partial | Charging is limited to 500 mA |
| PMIC (MT6357) | Partial | RTC reads a wrong year |
| Restart, power off | Yes | |
| Bluetooth, GPS, FM | No | |
| Camera, flashlight, vibrator | No | |

<details>
<summary>Details</summary>

- CPU: 4x Cortex-A53. cpuidle (core and cluster power-down), cpufreq 299 to 1274 MHz with `schedutil`.
- Clocks and regulators: `clk_ignore_unused` and `regulator_ignore_unused` are still on the kernel command line.
- eMMC: 192 MHz HS200. HS400 does not work yet.
- microSD: there is no card detect line, so the kernel finds the card only at boot.
- USB: serial shell and ECM networking through the gadget. PIO only, no DMA. No host mode.
- Display: DRM and fbcon through `mt6739-dbi`. PWM backlight. The panel sleeps when the screen is off.
- Touchscreen: Telink TLSC6x at I2C0 0x38, driven by `edt-ft5x06` (FT6x36 layout).
- Keypad: hold the hang-up key for a hardware reset.
- GPU: vendor DDK 1.9 firmware on the mainline `powervr` driver and Mesa. WebGL 1 works in Firefox. Sway still renders with pixman. See [docs/GPU.md](docs/GPU.md).
- Audio: own MT6739 AFE driver, `mt6357` codec, `aw8738` speaker amplifier, PipeWire with a UCM profile.
- Wi-Fi: CONSYS bring-up and the vendor gen2 driver, with NetworkManager and iwd. Firmware and NVRAM come from the phone's own partitions. See [docs/WIFI.md](docs/WIFI.md).
- Modem: `mt6739-ccci` driver, `ccci-fsd` NVRAM service and a ModemManager plugin. See [docs/MODEM-RE.md](docs/MODEM-RE.md).
- Battery: `mt6357-charger` fuel gauge (voltage, current, temperature, coulomb counting).
- Charger: external HL7005 on I2C1, 4.20 V. There is no USB charger type detection, so the input limit is always 500 mA.
- Restart and power off: PSCI hangs in the ATF, so restart goes through the watchdog and power off through the PMIC. With USB connected, power off goes to offline charging.

</details>

## How it boots

- LK, the stock bootloader, boots our `boot.img` from the `boot` partition. LineageOS stays in `recovery`.
- The NixOS initrd mounts the microSD card (GPT partition label `nixos`) as root.
- On the first boot, systemd-repart grows the root partition to the whole card.
- If the initrd fails, it writes the log to pstore and reboots into LineageOS.

## Requirements

- An AGM M7 with an unlocked bootloader. The stock `vendor`, `nvdata` and `protect` partitions must be intact.
- A microSD card of 4 GB or more.
- Nix with flakes and an `aarch64-linux` builder.
- `fastboot` for the first install.

## Install

1. Put your SSH public key in `nixos/configuration.nix`.

2. Write the image to the microSD card. On macOS:

   ```
   ./scripts/sd-flash.sh disk4
   ```

   Find the disk with `diskutil list external`. On Linux, write the output of `nix build .#packages.aarch64-linux.sd-image` with `dd`.

3. Build `boot.img` and flash it in fastboot mode:

   ```
   nix build .#packages.aarch64-linux.boot-img -o out/boot.img
   fastboot flash boot out/boot.img
   ```

4. Put the card in the phone and boot. Connect over USB: `ssh root@172.16.42.1`. The phone gives the host 172.16.42.2.

## Update

- `./scripts/nixos-deploy.sh switch` (now) or `boot` (at the next boot) copies the system over SSH. The bootloader module (`nixos/bootloader.nix`) writes the new `boot.img` to `boot` when the kernel or initrd changed.
- Kernel changes: push the kernel branch, then `nix flake update linux`, then deploy.
- Fast kernel development: `./scripts/build.sh` in an arm64 Linux VM, `KERNEL=vm ./scripts/mkboot-nix.sh`, then `./scripts/nixos-flash.sh`.

## Use

<details>
<summary>Interface and keys</summary>

- Sway (pixman renderer) runs as `user`, with `foot` and Firefox. Each app runs full screen on its own workspace. The home screen is workspace 1.
- Home screen (`agm-home`): time, date, battery, Wi-Fi and the mobile network. After 20 s idle the screen dims, after 30 s it locks and turns off. Any key turns it on.
- Keyguard (`agm-lock`): Menu then `*` unlocks, and also locks from anywhere.
- Settings (`agm-settings`): Wi-Fi, mobile network, sound, brightness, idle delays, battery, date and time, storage, phone info.
- Keys:
  - Left soft key: app menu (`agm-menu`). D-pad and OK, or 1 to 9. Back closes it.
  - Right soft key: Back.
  - Call key: next app.
  - Hang-up key: close the window.
  - Side key (F11): show or hide the `wvkbd` on-screen keyboard.
- Text input (`t9`):
  - 2 to 9 type a word. `*` or the D-pad selects the next word. A long press types the digit.
  - 0 types a space. 1 types punctuation, and 1 again selects the next mark.
  - `#` cycles the modes (T9, multi-tap, 123). A long press of `#` switches between English and Polish.
  - The right soft key deletes.
- Serial console: `/dev/cu.usbmodem*` (macOS) or `/dev/ttyACM*` (Linux), with root autologin.
- `systemctl reboot --reboot-argument=recovery` boots LineageOS.

</details>

## Layout

- `nixos/`: NixOS modules.
- `pkgs/`: the kernel, `boot.img` parts and `boot.img`.
- `tools/`: home screen, keyguard and settings (`shell`, Rust), the `t9` input method, the modem NVRAM service and ModemManager plugin, GPU test programs.
- `scripts/`: build, flash, deploy and test helpers.
- `docs/`: findings and reverse engineering notes.

## License

GPL-2.0-or-later, see `LICENSE`. Files with their own license header keep it.
