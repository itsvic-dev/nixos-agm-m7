#!/usr/bin/env bash
# usage: android-ui-bench.sh  (drives the Android UI over adb, then prints gfxinfo frame times per app)
set -euo pipefail
adb=${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}
apps=(com.android.systemui com.android.launcher3 com.android.settings com.android.wallpaper)
wallpaper=com.android.wallpaper/com.android.customization.picker.CustomizationPickerActivity

sh() { $adb shell "$@"; }
stats() {
	echo "== $1"
	sh dumpsys gfxinfo "$1" | grep -E 'Total frames|Janky frames:|(50|90|95|99)th percentile|Slow UI thread|Slow issue draw|(50|90)th gpu' || true
}
scroll() {
	for _ in 1 2 3; do sh input swipe 120 260 120 80 400; sleep 1; done
	for _ in 1 2 3; do sh input swipe 120 80 120 260 400; sleep 1; done
}

sh 'input keyevent WAKEUP; wm dismiss-keyguard; svc power stayon true; cmd statusbar collapse; input keyevent HOME'
sleep 2
for app in "${apps[@]}"; do sh dumpsys gfxinfo "$app" reset >/dev/null; done

echo "== quick settings"
for _ in 1 2 3 4 5; do
	sh cmd statusbar expand-settings; sleep 2
	sh cmd statusbar collapse; sleep 2
done

echo "== app drawer"
for _ in 1 2 3 4 5; do
	sh input keyevent KEYCODE_ALL_APPS; sleep 2
	sh input swipe 120 260 120 120 400; sleep 1
	sh input keyevent HOME; sleep 2
done

echo "== settings"
sh am start -W -a android.settings.SETTINGS >/dev/null; sleep 2
scroll
sh am start -W -a android.settings.DISPLAY_SETTINGS >/dev/null; sleep 2
scroll
settings_stats=$(stats com.android.settings)
sh input keyevent HOME; sleep 2

echo "== wallpaper picker"
sh am start -W -n $wallpaper >/dev/null; sleep 4
scroll
wallpaper_stats=$(stats com.android.wallpaper)
sh input keyevent HOME; sleep 2
sh svc power stayon false

stats com.android.systemui
stats com.android.launcher3
echo "$settings_stats"
echo "$wallpaper_stats"
echo "GPU resets: $(sh 'dmesg | grep -c "ddk19: HWR"' || true)"
