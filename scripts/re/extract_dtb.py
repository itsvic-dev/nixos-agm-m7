import struct, sys
f = open(sys.argv[1], 'rb').read()
ksz, = struct.unpack_from('<I', f, 8)
page, = struct.unpack_from('<I', f, 36)
k = f[page:page+ksz]
i = 0; n = 0
while (i := k.find(b'\xd0\x0d\xfe\xed', i)) >= 0:
    sz, = struct.unpack_from('>I', k, i+4)
    open(f'{sys.argv[2]}/dtb{n}.dtb', 'wb').write(k[i:i+sz])
    print(n, hex(i), sz); n += 1; i += sz
