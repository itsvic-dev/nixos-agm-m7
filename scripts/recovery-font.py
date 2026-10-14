#!/usr/bin/env python3
# usage: recovery-font.py <video_font_8x16.h> <out.png>  (minui font: 96 glyphs from ' ', regular row then bold row)
import re
import struct
import sys
import zlib

W, H = 8, 16

src = open(sys.argv[1]).read()
body = src[src.index('{', src.index('video_fontdata_8x16')):]
body = re.sub(r'/\*.*?\*/', '', body, flags=re.S)
data = bytes(int(x, 16) for x in re.findall(r'0x([0-9a-fA-F]{2})', body))
assert len(data) >= 256 * H, len(data)

rows = []
for bold in (False, True):
    for y in range(H):
        line = bytearray()
        for ch in range(32, 128):
            bits = data[ch * H + y]
            if bold:
                bits |= bits >> 1
            line += bytes(255 if bits & (0x80 >> x) else 0 for x in range(W))
        rows.append(bytes(line))

width = 96 * W
raw = b''.join(b'\0' + r for r in rows)


def chunk(kind, payload):
    return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))


png = b'\x89PNG\r\n\x1a\n'
png += chunk(b'IHDR', struct.pack('>IIBBBBB', width, 2 * H, 8, 0, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress(raw, 9))
png += chunk(b'IEND', b'')
open(sys.argv[2], 'wb').write(png)
