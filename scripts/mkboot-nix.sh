#!/usr/bin/env bash
# usage: [KERNEL=vm] mkboot-nix.sh  (vm: Image.gz and DTB from the VM build in out/)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=$here/out
parts=$out/boot-parts
table=$here/secrets/userdata-dm-table.txt
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

nix build "$here#packages.aarch64-linux.boot-parts" -o "$parts"
image=$parts/Image.gz
dtb=$parts/mt6739-agm-m7.dtb
if [ "${KERNEL:-}" = vm ]; then
	image=$out/arch/arm64/boot/Image.gz
	dtb=$out/arch/arm64/boot/dts/mediatek/mt6739-agm-m7.dtb
fi

mkdir -p "$tmp/root/etc"
awk '{ $7 = "/dev/mmcblk0p33"; print }' "$table" > "$tmp/root/etc/userdata.table"
(cd "$tmp/root" && find . | cpio --quiet -o -H newc -R 0:0 | gzip -9 -n) > "$tmp/secret.cpio.gz"
cat "$parts/initrd" "$tmp/secret.cpio.gz" > "$tmp/ramdisk"

python3 "$here/scripts/mkboot.py" \
	--kernel "$image" \
	--dtb "$dtb" \
	--ramdisk "$tmp/ramdisk" \
	--cmdline "$(cat "$parts/cmdline")" \
	-o "$out/boot.img"
size=$(wc -c < "$out/boot.img")
max=25165824
[ "$size" -le "$max" ] || { echo "boot.img is $size bytes, recovery holds $max"; exit 1; }
ls -l "$out/boot.img"
