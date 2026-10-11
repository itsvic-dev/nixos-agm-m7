#!/usr/bin/env bash
# usage: nixos-push.sh [size]
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
export PATH=$HOME/Library/Android/sdk/platform-tools:$PATH
size=${1:-3G}
img=$here/out/nixos.img

nix build "$here#packages.aarch64-linux.rootfs" -o "$here/out/nixos-rootfs"
install -m 644 "$here/out/nixos-rootfs" "$img"
truncate -s "$size" "$img"
nix shell nixpkgs#e2fsprogs -c sh -c "e2fsck -fy '$img' >/dev/null; resize2fs '$img'"
adb shell "phh-su -c 'rm -f /data/nixos.img /data/local/tmp/nixos.img'"
adb push "$img" /data/local/tmp/nixos.img
adb shell "phh-su -c 'mv /data/local/tmp/nixos.img /data/nixos.img'"
