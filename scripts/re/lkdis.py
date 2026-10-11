import struct, sys
from capstone import *
d = open(sys.argv[1], 'rb').read()[0x200:]
base = 0x46000000
start, end = int(sys.argv[2], 16), int(sys.argv[3], 16)
md = Cs(CS_ARCH_ARM, CS_MODE_THUMB)
md.skipdata = True
def s_at(a):
    o = a - base
    if 0 <= o < len(d):
        e = d.find(b'\0', o)
        t = d[o:e]
        if 3 < len(t) < 100 and all(32 <= c < 127 or c in (9, 10) for c in t):
            return repr(t.decode())
    return ''
regs = {}
for i in md.disasm(d[start - base:end - base], start):
    m, ops = i.mnemonic, i.op_str
    extra = ''
    try:
        if m.startswith('ldr') and '[pc, #' in ops:
            r = ops.split(',')[0]
            o = int(ops.split('#')[1].rstrip(']'), 16)
            v = struct.unpack_from('<I', d, ((i.address + 4) & ~3) + o - base)[0]
            regs[r] = v
            extra = ' ; =%#x' % v
        elif m.startswith('add') and ops.endswith(', pc') and ops.count(',') == 1:
            r = ops.split(',')[0]
            if r in regs:
                val = (regs[r] + i.address + 4) & 0xffffffff
                extra = ' ; %#x %s' % (val, s_at(val))
    except (ValueError, struct.error):
        pass
    print('%08x %-8s %s%s' % (i.address, m, ops, extra))
