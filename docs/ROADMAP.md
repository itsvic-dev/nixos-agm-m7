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
- Graphical session: Sway (pixman) with `foot`, `fuzzel` and Firefox. The `wvkbd` on-screen keyboard stays hidden until the right soft key shows it.
- Fast test loop: flash `recovery` from NixOS and `reboot recovery` through the RTC boot flag.
- Wi-Fi: CONSYS bring-up and the vendor gen2 driver in staging, with iwd (see `WIFI.md`).
- GPU: vendor DDK 1.9 firmware on the mainline `powervr` driver, Mesa Vulkan 1.2, and GL 2.1 / GLES 2.0 through Zink (see `GPU.md`).

## Now

- T9 input method (`tools/t9`): passes the headless Sway check, not yet tested on the phone.

## Next

Nothing. Pick from the follow-ups.

## Follow-ups

- Root on an SD card (MSDC1, mainline `mtk-sd`). It keeps USB free for serial and networking, and the encrypted `userdata` image can go.
- USB host mode: VBUS boost in the HL7005 driver (`OPA_MODE`) and role switching through the WUSB3801. Host mode removes USB serial and networking while in use.
- Shrink the initrd (OpenSSL, libxml2, Kerberos and more are still in), so `boot.img` has more room in `recovery`.
- Drop the `userdata` encryption once updating and reflashing from NixOS is reliable. Reformatting `userdata` deletes the LineageOS data, so ask first.
- Remove `regulator_ignore_unused` from the cmdline. First find which regulators must stay on.
- RTC: Linux reads the date as 2069-12-31. Check the year mapping against the stock driver (base year 1968).
- Offline charging: LK boots the `boot` partition (LineageOS) with the charger mode. There, Android init finds no `/system` binaries (no healthd, no charger UI), so the screen keeps LK's picture. The battery still charges. A long press gets out of it. Fix it when NixOS replaces `boot`: handle `androidboot.mode=charger` in NixOS.
- auxadc: `batt_v` reads garbage and some channels time out.
- Charger: BC1.2 charger type detection for higher currents on wall chargers.
- Remove `clk_ignore_unused` from the cmdline.
- USB DMA (MUSB runs in PIO mode).
- eMMC HS400: find the strobe delay register in the MSDC0 pad macro (0x11cd0000) and give `mtk-sd` an MT6739 entry for it. A failed HS400 tuning leaves the eMMC unusable, so test with care. HS200 already gives most of the speed.
- NixOS: clean shutdown of the loop-mounted root.
- `MEMCG` crashes in `__refill_obj_stock` when systemd loads BPF.
- Display: panel power and init without LK, TE or a vblank event, DBI frame-done IRQ instead of polling.
- macOS sometimes does not create the USB network interface. A cable replug fixes it.
- T9: learn new words, number mode for number fields (`content_type`), keep `wvkbd` keys out of T9.
- GPU: HWR recovery after a lockup, GLES 3 (needs transform feedback), hardware WebRender in Firefox, Sway on the GPU instead of pixman. See `GPU.md`.
- SD card, Bluetooth, audio, vibrator.
