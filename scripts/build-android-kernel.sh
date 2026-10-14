#!/usr/bin/env bash
# usage: [LINEAGE=/Volumes/Android/lineage] build-android-kernel.sh  (in the VM: kernel for the LineageOS device tree in out-android/pkg)
set -euo pipefail
cd /Volumes/Projects/agm-m7-work
export KOUT=out-android
los=${LINEAGE:-/Volumes/Android/lineage}
frag=out-android/android.frag
mkdir -p out-android
cat "$los"/kernel/configs/b/android-6.12/android-base.config \
	"$los"/kernel/mainline/configs/fragments/android-base-conditional/{CONFIG_ARM64-y,CONFIG_EXT4_FS-y,CONFIG_F2FS_FS-y}.config \
	"$los"/kernel/mainline/configs/fragments/android-base-pre/{common,arm64}.config \
	"$los"/device/agm/m7/configs/kernel/android.config > $frag
scripts/kmake allnoconfig >/dev/null
scripts/kmake agm-m7.config >/dev/null 2>&1
linux/scripts/kconfig/merge_config.sh -m -O out-android out-android/.config $frag >/dev/null
scripts/kmake olddefconfig >/dev/null
missing=0
while read -r line; do
	case "$line" in ""|"#"*) [ "${line#\# CONFIG_}" = "$line" ] && continue ;; esac
	grep -qxF "$line" out-android/.config || { echo "not applied: $line"; missing=1; }
done < $frag
echo "== fragment check: $([ $missing = 0 ] && echo ok || echo MISSING)"
scripts/kmake Image.gz modules mediatek/mt6739-agm-m7.dtb
rm -rf out-android/pkg && mkdir -p out-android/pkg
scripts/kmake modules_install INSTALL_MOD_PATH=$PWD/out-android/pkg/mods INSTALL_MOD_STRIP=1 >/dev/null
cp out-android/arch/arm64/boot/dts/mediatek/mt6739-agm-m7.dtb out-android/pkg/m7.dtb
fdtput -p -c out-android/pkg/m7.dtb /firmware/android
fdtput -t s out-android/pkg/m7.dtb /firmware/android compatible android,firmware
fdtput -t s out-android/pkg/m7.dtb /firmware/android boot_devices soc/11240000.mmc
cat out-android/arch/arm64/boot/Image.gz out-android/pkg/m7.dtb > out-android/pkg/Image.gz-dtb
find out-android/pkg/mods -name "*.ko" | wc -l | sed "s/^/modules: /"
ls -l out-android/pkg/Image.gz-dtb
dest=$los/device/agm/m7-kernel
rm -rf "$dest" && mkdir -p "$dest"
cp out-android/pkg/Image.gz-dtb "$dest"/
find out-android/pkg/mods -name "*.ko" ! -name "clk-mt8196-*" ! -name "mediatek-cpufreq-hw.ko" ! -name "sit.ko" -exec cp {} "$dest"/ \;
echo "installed to $dest: $(ls "$dest" | wc -l) files"
