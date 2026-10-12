#!/usr/bin/env bash
# usage: [KERNEL=vm] mkboot-nix.sh  (vm: Image.gz and DTB from the VM build in out/)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=$here/out
parts=$out/boot-parts

nix build "$here#packages.aarch64-linux.boot-parts" -o "$parts"
image=$parts/Image.gz
dtb=$parts/mt6739-agm-m7.dtb
if [ "${KERNEL:-}" = vm ]; then
	image=$here/${KOUT:-out}/arch/arm64/boot/Image.gz
	dtb=$here/${KOUT:-out}/arch/arm64/boot/dts/mediatek/mt6739-agm-m7.dtb
fi

python3 "$here/scripts/mkboot.py" \
	--kernel "$image" \
	--dtb "$dtb" \
	--ramdisk "$parts/initrd" \
	--cmdline "$(cat "$parts/cmdline")" \
	-o "$out/boot.img"
size=$(wc -c < "$out/boot.img")
max=25165824
[ "$size" -le "$max" ] || { echo "boot.img is $size bytes, the boot partition holds $max"; exit 1; }
ls -l "$out/boot.img"
