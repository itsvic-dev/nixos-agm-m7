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
python3 "$here/scripts/mkboot.py" --kernel "$out/u-boot.bin.gz" --dtb "$parts/mt6739-agm-m7.dtb" -o "$out/boot.img"
ls -l "$out/u-boot.bin" "$out/boot.img"
