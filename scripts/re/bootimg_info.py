import struct, sys
f = open(sys.argv[1], 'rb').read()
assert f[:8] == b'ANDROID!', f[:8]
(ksz, kaddr, rsz, raddr, ssz, saddr, tags, page, hv, osv) = struct.unpack_from('<10I', f, 8)
name = f[48:64].rstrip(b'\0')
cmd = f[64:64+512].rstrip(b'\0')
print(f'header_version={hv} page={page:#x} os_version={osv:#x}')
print(f'kernel size={ksz:#x} addr={kaddr:#x}')
print(f'ramdisk size={rsz:#x} addr={raddr:#x}')
print(f'second size={ssz:#x} addr={saddr:#x} tags={tags:#x}')
print(f'name={name!r}')
print(f'cmdline={cmd!r}')
koff = page
k = f[koff:koff+ksz]
print('kernel magic', k[:4].hex(), 'gzip' if k[:2]==b'\x1f\x8b' else '')
d = k.find(b'\xd0\x0d\xfe\xed')
print('first FDT magic in kernel blob at', hex(d) if d >= 0 else None)
mtk = k[:4] == b'\x88\x16\x88\x58'
print('MTK header on kernel:', mtk, k[8:40].rstrip(b"\0") if mtk else '')
