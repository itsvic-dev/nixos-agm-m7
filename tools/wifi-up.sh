#!/usr/bin/env bash
# usage: wifi-up.sh [dir]  (on the phone; dir has the firmware, wmt-init and the two modules)
set -euo pipefail
dir=${1:-/root/fw}

echo -n "$dir" > /sys/module/firmware_class/parameters/path
grep -q '^cfg80211 ' /proc/modules || insmod "$dir/cfg80211.ko"
grep -q '^mt6739_wlan ' /proc/modules || insmod "$dir/mt6739-wlan.ko"
echo 1 > /sys/kernel/debug/mt6739-consys/power
(cd "$dir" && ./wmt-init)
echo 1 > /sys/module/mt6739_wlan/parameters/wlan_on
systemctl restart iwd
