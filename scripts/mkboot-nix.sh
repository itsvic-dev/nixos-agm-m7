#!/usr/bin/env bash
# usage: mkboot-nix.sh  (writes out/boot.img from the flake build)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=$here/out
parts=$out/boot-parts
table=$here/secrets/userdata-dm-table.txt
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

nix build "$here#packages.aarch64-linux.boot-parts" -o "$parts"

mkdir -p "$tmp/root/etc"
awk '{ $7 = "/dev/mmcblk0p33"; print }' "$table" > "$tmp/root/etc/userdata.table"
(cd "$tmp/root" && find . | cpio --quiet -o -H newc -R 0:0 | gzip -9 -n) > "$tmp/secret.cpio.gz"
cat "$parts/initrd" "$tmp/secret.cpio.gz" > "$tmp/ramdisk"

python3 "$here/scripts/mkboot.py" \
	--kernel "$parts/Image.gz" \
	--dtb "$parts/mt6739-agm-m7.dtb" \
	--ramdisk "$tmp/ramdisk" \
	--cmdline "$(cat "$parts/cmdline")" \
	-o "$out/boot.img"
ls -l "$out/boot.img"
