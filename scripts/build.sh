#!/usr/bin/env bash
# usage: build.sh  (in the VM: Image.gz and the DTB in out/, then KERNEL=vm scripts/mkboot-nix.sh on the host)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)

"$here/scripts/kmake" Image.gz mediatek/mt6739-agm-m7.dtb
ls -l "$here/out/arch/arm64/boot/Image.gz"
