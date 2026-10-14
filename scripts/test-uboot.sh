#!/usr/bin/env bash
# usage: test-uboot.sh [boot.img]  (from NixOS: write U-Boot to recovery, boot it, print its log after it resets to NixOS)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
img=${1:-$here/out-uboot/boot.img}
host=root@172.16.42.1

scp -q "$img" "$host:/tmp/uboot.img"
ssh "$host" 'dd if=/tmp/uboot.img of=/dev/disk/by-partlabel/recovery bs=1M conv=fsync status=none &&
	cmp -n $(stat -c %s /tmp/uboot.img) /tmp/uboot.img /dev/disk/by-partlabel/recovery &&
	systemctl reboot --reboot-argument=recovery'
echo "== rebooted into recovery at $(date +%T), waiting for NixOS"
sleep 20
for _ in $(seq 60); do
	ssh -o ConnectTimeout=3 -o BatchMode=yes "$host" true 2>/dev/null && break
	sleep 5
done
echo "== NixOS is up at $(date +%T)"
ssh "$host" ramconsole | awk '/<debug_uart>|U-Boot 20/ { last = NR } { line[NR] = $0 } END { if (!last) { print "no U-Boot output"; exit } for (i = last; i <= NR && line[i] !~ /Booting Linux/; i++) print line[i] }'
