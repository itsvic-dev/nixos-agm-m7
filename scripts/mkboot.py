#!/usr/bin/env python3
import argparse
import struct

PAGE = 0x800
RAMDISK_ADDR = 0x45000000
LK_ADDR = 0x46000000


def pad(b):
    return b + b'\0' * (-len(b) % PAGE)


ap = argparse.ArgumentParser(description='Pack an MTK LK compatible v0 boot.img')
ap.add_argument('--kernel', required=True, help='Image.gz (arm64) or zImage (arm)')
ap.add_argument('--dtb', required=True)
ap.add_argument('--ramdisk')
ap.add_argument('--arm32', action='store_true')
ap.add_argument('--cmdline', default='')
ap.add_argument('-o', '--output', required=True)
a = ap.parse_args()

kernel = open(a.kernel, 'rb').read() + open(a.dtb, 'rb').read()
ramdisk = open(a.ramdisk, 'rb').read() if a.ramdisk else b''
bootopt = 'bootopt=64S3,32S1,32S1' if a.arm32 else 'bootopt=64S3,32N2,64N2'
cmdline = f'{bootopt} {a.cmdline}'.strip().encode()
assert len(cmdline) < 512
if len(ramdisk) > LK_ADDR - RAMDISK_ADDR:
    raise SystemExit(f'ramdisk is {len(ramdisk)} bytes, LK at {LK_ADDR:#x} leaves {LK_ADDR - RAMDISK_ADDR}')

hdr = struct.pack(
    '<8s10I16s512s32s1024s',
    b'ANDROID!',
    len(kernel), 0x40008000 if a.arm32 else 0x40200000,
    len(ramdisk), RAMDISK_ADDR,
    0, 0x40f00000,
    0x44000000,
    PAGE,
    0, 0,
    b'', cmdline, b'', b'',
)
with open(a.output, 'wb') as f:
    f.write(pad(hdr))
    f.write(pad(kernel))
    if ramdisk:
        f.write(pad(ramdisk))
