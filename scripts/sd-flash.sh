#!/usr/bin/env bash
# usage: sd-flash.sh <disk>  (macOS: write the NixOS SD image to a card, e.g. disk4)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
disk=${1#/dev/}

info=$(diskutil info "$disk")
grep -qE "Removable Media: +Removable" <<<"$info" || { echo "$disk is not removable media"; exit 1; }
grep -E "Device / Media Name|Disk Size" <<<"$info"
read -rp "Erase $disk and write the NixOS image? [y/N] " ok
[ "$ok" = y ] || exit 1

nix build "$here#packages.aarch64-linux.sd-image" -o "$here/out/sd-image"
diskutil unmountDisk "$disk"
sudo dd if="$here/out/sd-image" of="/dev/r$disk" bs=4m status=progress
sync
diskutil eject "$disk"
