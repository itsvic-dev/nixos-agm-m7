#!/usr/bin/env bash
# usage: android-flash.sh  (in android-build-arm64: sparse super.img to the microSD super partition, boot.img to recovery; boot it with "reboot recovery")
set -euo pipefail
host=${HOST:-root@172.16.42.1}
out=${OUT:-/Volumes/Android/lineage/out/target/product/m7}
stage=/var/cache/android-flash
cd "$out"

read -r size < <(python3 - boot.img super.img <<'EOF'
import struct, sys
b = open(sys.argv[1], 'rb').read(64)
assert b[:8] == b'ANDROID!', 'not a boot.img'
ks, ka, rs, ra = struct.unpack_from('<4I', b, 8)
hv = struct.unpack_from('<I', b, 40)[0]
assert hv == 0 and ka == 0x40200000 and ra == 0x45000000, f'header v{hv} kernel {ka:#x} ramdisk {ra:#x}'
assert rs <= 0x46000000 - 0x45000000, f'ramdisk {rs} bytes overlaps LK'
s = open(sys.argv[2], 'rb').read(28)
magic, _, _, _, _, blk, blocks = struct.unpack_from('<IHHHHII', s)
assert magic == 0xed26ff3a, 'super.img is not sparse'
print(blk * blocks)
print(f'boot.img ok: kernel {ks}, ramdisk {rs}', file=sys.stderr)
EOF
)
bsize=$(stat -c %s boot.img)
bsum=$(sha256sum boot.img | cut -d' ' -f1)

ssh "$host" "set -e
[ -e /dev/disk/by-partlabel/super ] || { echo 'no super partition'; exit 1; }
[ \"\$(blockdev --getsize64 /dev/disk/by-partlabel/super)\" -ge $size ] || { echo 'super.img is larger than super'; exit 1; }
[ \"\$(blockdev --getsize64 /dev/disk/by-partlabel/recovery)\" -ge $bsize ] || { echo 'boot.img is larger than recovery'; exit 1; }
mkdir -p $stage"

echo "== copy to $host:$stage"
rsync --info=progress2 super.img boot.img "$host:$stage/"

echo "== write super (data blocks only)"
ssh "$host" "time simg2img $stage/super.img /dev/disk/by-partlabel/super && sync && lpdump /dev/disk/by-partlabel/super | sed -n '/^Partition table/,/^Super partition layout/p'"

echo "== write recovery"
ssh "$host" "dd if=$stage/boot.img of=/dev/disk/by-partlabel/recovery bs=1M conv=fsync status=progress && echo 3 > /proc/sys/vm/drop_caches"
bgot=$(ssh "$host" "head -c $bsize /dev/disk/by-partlabel/recovery | sha256sum | cut -d' ' -f1")
[ "$bgot" = "$bsum" ] || { echo "recovery checksum mismatch: $bgot"; exit 1; }
echo "flashed. Boot Android with: ssh $host systemctl reboot --reboot-argument=recovery"
