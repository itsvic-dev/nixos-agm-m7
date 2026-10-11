import struct, sys
from capstone import *
d = open(sys.argv[1], 'rb').read()[0x200:]
base = 0x46000000
targets = {int(x, 16) for a in sys.argv[2:] for x in a.split()}
md = Cs(CS_ARCH_ARM, CS_MODE_THUMB)
md.skipdata = True
regs = {}
for i in md.disasm(d, base):
    m, ops = i.mnemonic, i.op_str
    try:
        if m.startswith('ldr') and '[pc, #' in ops:
            r = ops.split(',')[0]
            o = int(ops.split('#')[1].rstrip(']'), 16)
            a = ((i.address + 4) & ~3) + o
            v = struct.unpack_from('<I', d, a - base)[0]
            regs[r] = (v, i.address)
            if v in targets:
                print('lit %#x -> %#x' % (i.address, v))
        elif m.startswith('movw'):
            r, v = ops.split(', #')
            regs[r] = (int(v, 16), i.address)
        elif m.startswith('movt'):
            r, v = ops.split(', #')
            if r in regs:
                val = (int(v, 16) << 16) | (regs[r][0] & 0xffff)
                regs[r] = (val, regs[r][1])
                if val in targets:
                    print('movt %#x -> %#x' % (regs[r][1], val))
        elif m.startswith('add') and ops.endswith(', pc') and ops.count(',') == 1:
            r = ops.split(',')[0]
            if r in regs:
                val = (regs[r][0] + i.address + 4) & 0xffffffff
                if val in targets:
                    print('pcrel %#x -> %#x' % (regs[r][1], val))
    except (ValueError, struct.error):
        pass
