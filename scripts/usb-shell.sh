#!/usr/bin/env bash
# usage: usb-shell.sh <unused> <timeout s> <log file> [commands...]
set -euo pipefail
before=$1 timeout=$2 log=$3
shift 3

sleep 20
dev=""
for i in $(seq 1 "$timeout"); do
	for d in $(ls /dev/cu.usbmodem* 2>/dev/null || true); do
		if (exec 3<>"$d") 2>/dev/null; then
			dev=$d
			break 2
		fi
	done
	sleep 1
done
[ -n "$dev" ] || { echo "no new USB serial device"; exit 1; }
echo "found $dev"

sleep 2
exec 3<>"$dev"
stty -f "$dev" raw -echo 115200
cat <&3 > "$log" &
reader=$!
sleep 1
printf '\n' >&3
for c in "$@"; do
	printf '%s\n' "$c" >&3
	sleep 2
done
sleep 3
kill $reader 2>/dev/null || true
cat "$log"
