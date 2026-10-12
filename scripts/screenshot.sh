#!/usr/bin/env bash
# usage: screenshot.sh [file]  (default: out/screenshots/<time>.png)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
host=root@172.16.42.1
opts=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes)
file=${1:-$here/out/screenshots/$(date +%Y%m%d-%H%M%S).png}

mkdir -p "$(dirname "$file")"
ssh "${opts[@]}" "$host" 'XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 runuser -u user -- grim -' > "$file"
echo "$file"
