# AGM M7 roadmap

Findings are in `NOTES.md`. This file tracks what is done and what is next.

## Done

- Boot: LK loads our `boot.img` from `recovery`, ODM overlay sink, pstore earlycon, early watchdog.
- Restart through the `mtk_wdt` restart handler.
- Clocks, pinctrl, GPIO, EINT.
- USB gadget: serial shell and ECM networking.
- eMMC, encrypted `userdata` (dm-crypt, f2fs).
- postmarketOS rootfs from a loop image on `/data`.
- MT6357 PMIC: pwrap, regulators, RTC, auxadc.
- Keypad matrix and PMIC power key.
- Battery voltage from the auxadc.
- Power off through the PMIC.
- NixOS 26.05 userspace from a flake in this repo.
- Display: DRM driver for the DBI path that LK sets up, with fbcon.
- Battery: fuel gauge (current, coulomb counting, temperature), charger detection.
- I2C0 and I2C1, touchscreen, HL7005 charger.
- Flake builds the kernel (from GitHub) and `boot.img` parts. NixOS systemd initrd replaces the busybox initramfs.
- Graphical session: `labwc` with `foot` and the `wvkbd` on-screen keyboard.
- Fast test loop: flash `recovery` from NixOS and `reboot recovery` through the RTC boot flag.

## Now

Nothing. Pick from the follow-ups.

## Follow-ups

- Shrink the initrd (OpenSSL, libxml2, Kerberos and more are still in), so `boot.img` has more room in `recovery`.
- Drop the `userdata` encryption once updating and reflashing from NixOS is reliable. Reformatting `userdata` deletes the LineageOS data, so ask first.
- Remove `regulator_ignore_unused` from the cmdline. First find which regulators must stay on.
- RTC: Linux reads the date as 2069-12-31. Check the year mapping against the stock driver (base year 1968).
- Long-press reset does not work on mainline (a hung power off needed a battery pull). Check the PMIC key settings.
- auxadc: `batt_v` reads garbage and some channels time out.
- Charger: BC1.2 charger type detection for higher currents on wall chargers.
- Remove `clk_ignore_unused` from the cmdline.
- USB DMA (MUSB runs in PIO mode).
- NixOS: build the kernel and `boot.img` in the flake. Maybe use Mobile NixOS parts. Not for upstream.
- NixOS: clean shutdown of the loop-mounted root.
- `MEMCG` crashes in `__refill_obj_stock` when systemd loads BPF.
- Display: backlight control (`disp_pwm0`), panel power and init without LK, TE or a vblank event, DBI frame-done IRQ instead of polling.
- macOS sometimes does not create the USB network interface. A cable replug fixes it.
- SD card, Wi-Fi, Bluetooth, audio, vibrator, GPU.
