#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=$here/out
rm -rf "$out/initramfs"
mkdir -p "$out/initramfs/bin"
cp "$here/initramfs/init" "$out/initramfs/init"
cp /bin/busybox "$out/initramfs/bin/busybox"
gcc -O2 -static -Wall -o "$out/dmctl" "$here/tools/dmctl.c"
cp "$out/dmctl" "$out/initramfs/bin/dmctl"
if [ -f "$here/secrets/userdata-dm-table.txt" ]; then
	mkdir -p "$out/initramfs/etc"
	awk '{ $7 = "/dev/mmcblk0p33"; print }' "$here/secrets/userdata-dm-table.txt" > "$out/initramfs/etc/userdata.table"
fi
(cd "$out/initramfs" && find . | cpio --quiet -o -H newc -R 0:0 | gzip -9) > "$out/initramfs.cpio.gz"

"$here/scripts/kmake" Image.gz mediatek/mt6739-agm-m7.dtb

python3 "$here/scripts/mkboot.py" \
	--kernel "$out/arch/arm64/boot/Image.gz" \
	--dtb "$out/arch/arm64/boot/dts/mediatek/mt6739-agm-m7.dtb" \
	--ramdisk "$out/initramfs.cpio.gz" \
	--cmdline "earlycon=pstoreram,0x4445f000 keep_bootcon mtk_wdt_early=0x10007000,30 clk_ignore_unused regulator_ignore_unused loglevel=8 rdinit=/init g_cdc.dev_addr=02:4d:37:00:00:01 g_cdc.host_addr=02:4d:37:00:00:02" \
	-o "$out/boot.img"
ls -l "$out/boot.img"
