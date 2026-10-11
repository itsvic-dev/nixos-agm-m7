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
- Fast test loop: flash `recovery` from NixOS and `reboot recovery` through the RTC boot flag.

## Now

1. I2C (the controller does not work yet), then the HL7005 charger on I2C1 and the TLSC6x touchscreen on I2C0.
2. Build the kernel and `boot.img` in the flake.
3. Weston.
4. Touchscreen.

## Follow-ups

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
