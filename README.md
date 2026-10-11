# AGM M7 mainline

Mainline Linux on the AGM M7 (MediaTek MT6739). The kernel tree is in `linux/` (branch `agm-m7`, a separate repo). See `docs/NOTES.md` for the findings.

## Layout

- `scripts/`: build, flash and test helpers.
- `scripts/re/`: LK and boot.img analysis helpers.
- `tools/`: small helpers for device-mapper.
- `initramfs/init`: busybox init.
- `stock/`: decompiled stock DT and ODM DTBO.

## Build

In the `agm-m7` OrbStack VM:

```
./scripts/config.sh
./scripts/build.sh
```

This writes `out/boot.img`.
