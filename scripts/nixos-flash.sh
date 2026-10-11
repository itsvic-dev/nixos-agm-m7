#!/usr/bin/env bash
# usage: nixos-flash.sh  (from a running NixOS: write out/boot.img to recovery and boot it)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
host=root@172.16.42.1
opts=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes)

scp "${opts[@]}" "$here/out/boot.img" "$host:/tmp/boot.img"
ssh "${opts[@]}" "$host" 'dd if=/tmp/boot.img of=/dev/disk/by-partlabel/recovery bs=1M conv=fsync && systemctl reboot --reboot-argument=recovery'
