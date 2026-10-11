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

## Now

1. Replace the pmOS userspace with (Mobile) NixOS, as a flake in this repo. Not for upstream.

## Next

1. Display: a DBI command-mode panel (GC9306, 240x320), at least simple DRM.

## Follow-ups

- Remove `regulator_ignore_unused` from the cmdline. First find which regulators must stay on.
- RTC: Linux reads the date as 2069-12-31. Check the year mapping against the stock driver (base year 1968).
- Long-press reset does not work on mainline (a hung power off needed a battery pull). Check the PMIC key settings.
- auxadc: `batt_v` reads garbage and some channels time out.
- Battery: charger and fuel gauge. Mainline has no MT6357 driver for them.
- Remove `clk_ignore_unused` from the cmdline.
- USB DMA (MUSB runs in PIO mode).
- SD card, Wi-Fi, Bluetooth, audio, vibrator, backlight, GPU.
