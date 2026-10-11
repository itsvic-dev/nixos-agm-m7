#!/usr/bin/env bash
# usage: nixos-push.sh [size]
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
export PATH=$HOME/Library/Android/sdk/platform-tools:$PATH
size=${1:-3G}
su() { adb shell "phh-su -c '$*'"; }

nix build "$here#packages.aarch64-linux.rootfs" -o "$here/out/nixos-rootfs"
adb push "$here/out/nixos-rootfs" /data/local/tmp/nixos.img
su "mv /data/local/tmp/nixos.img /data/nixos.img && truncate -s $size /data/nixos.img && e2fsck -fy /data/nixos.img >/dev/null; resize2fs /data/nixos.img 2>&1 | tail -1"
