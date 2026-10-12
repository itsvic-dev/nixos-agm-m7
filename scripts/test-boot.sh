#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
export PATH=$HOME/Library/Android/sdk/platform-tools:$PATH
name=${1:?usage: test-boot.sh <log name>}
logs=$here/logs/$name
mkdir -p "$logs"

wait_adb() {
	until [ "$(adb get-state 2>/dev/null)" = device ] &&
		[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do
		sleep 2
	done
}

wait_adb
adb push "$here/out/boot.img" /data/local/tmp/agm-boot.img >/dev/null
adb shell "phh-su -c 'dd if=/data/local/tmp/agm-boot.img of=/dev/block/platform/bootdevice/by-name/boot bs=1M conv=fsync' && echo flashed" | grep -q flashed ||
	{ echo "flashing boot failed"; exit 1; }

ls /dev/cu.usbmodem* 2>/dev/null > "$logs/usb-before.txt" || true
adb reboot
start=$(date +%s)
IFS=$'\n' read -r -d '' -a cmds <<< "${USB_CMDS:-$'uname -a\nreboot -f'}" || true
if [ -n "${USB:-}" ]; then
	"$here/scripts/usb-shell.sh" "$logs/usb-before.txt" 90 "$logs/usb-shell.txt" \
		"${cmds[@]}" || true
fi
eval "${AFTER_USB:-true}"
sleep 10
echo "waiting for LineageOS: it boots only with reboot recovery"
wait_adb
echo "back in LineageOS after $(( $(date +%s) - start ))s"
sleep 5

for f in $(adb shell "phh-su -c 'ls /sys/fs/pstore'" | tr -d '\r'); do
	adb shell "phh-su -c 'cat /sys/fs/pstore/$f'" > "$logs/$f.txt"
done
ls -l "$logs"
