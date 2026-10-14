#!/usr/bin/env bash
# usage: build-uboot.sh  (on the host: builds u-boot/ in the agm-m7 VM, packs out-uboot/boot.img for LK)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=$here/out-uboot

orb run -m agm-m7 sh -c "set -e
make -C '$here/u-boot' O='$out' mt6739_agm_m7_defconfig >/dev/null
make -C '$here/u-boot' O='$out' -j\$(nproc) u-boot.bin"

parts=$(nix build "$here#packages.aarch64-linux.boot-parts" --no-link --print-out-paths)
gzip -9 -n -c "$out/u-boot.bin" > "$out/u-boot.bin.gz"
# LK reads the last 512 KiB of the kernel blob to find the DTB, so a smaller blob fails to boot.
truncate -s 1M "$out/u-boot.bin.gz"
# LK does not start a boot.img without a ramdisk, so add an empty cpio.
python3 -c "
import gzip, sys
name = b'TRAILER!!!\\0'
h = b'070701' + b''.join(b'%08X' % v for v in [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, len(name), 0]) + name
h += b'\\0' * (-len(h) % 512)
open(sys.argv[1], 'wb').write(gzip.compress(h, mtime=0))
" "$out/ramdisk.cpio.gz"
python3 "$here/scripts/mkboot.py" --kernel "$out/u-boot.bin.gz" --dtb "$parts/mt6739-agm-m7.dtb" --ramdisk "$out/ramdisk.cpio.gz" -o "$out/boot.img"
ls -l "$out/u-boot.bin" "$out/boot.img"
