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
- Battery percent from the stock OCV table, charger detection.

## Now

1. Build the kernel and `boot.img` in the flake.
2. Weston.
3. Touchscreen.

## Follow-ups

- Remove `regulator_ignore_unused` from the cmdline. First find which regulators must stay on.
- RTC: Linux reads the date as 2069-12-31. Check the year mapping against the stock driver (base year 1968).
- Long-press reset does not work on mainline (a hung power off needed a battery pull). Check the PMIC key settings.
- auxadc: `batt_v` reads garbage and some channels time out.
- Battery: charging control and a fuel gauge (coulomb counter). Mainline has no MT6357 driver for them.
- Remove `clk_ignore_unused` from the cmdline.
- USB DMA (MUSB runs in PIO mode).
- NixOS: build the kernel and `boot.img` in the flake. Maybe use Mobile NixOS parts. Not for upstream.
- NixOS: clean shutdown of the loop-mounted root.
- `MEMCG` crashes in `__refill_obj_stock` when systemd loads BPF.
- Display: backlight control (`disp_pwm0`), panel power and init without LK, TE or a vblank event, DBI frame-done IRQ instead of polling.
- macOS sometimes does not create the USB network interface. A cable replug fixes it.
- SD card, Wi-Fi, Bluetooth, audio, vibrator, GPU.
