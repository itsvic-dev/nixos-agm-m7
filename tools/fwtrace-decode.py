#!/usr/bin/env python3
# usage: fwtrace-decode.py rgx_fwif_sf.h < ddk19/state
import re
import sys

GROUPS = ["NULL", "MAIN", "CLEANUP", "CSW", "PM", "RTD", "SPM", "MTS", "BIF", "DATA", "POW",
          "HWR", "HWP", "RPM", "DMA", "DBG"]


def load(path):
    text = open(path, errors="replace").read()
    table = {}
    for m in re.finditer(r'X\(\s*(\d+)\s*,\s*RGXFW_GROUP_(\w+)\s*,\s*\w+\s*,\s*"((?:[^"\\]|\\.)*)"\s*,\s*(\d+)\)', text):
        sf_id, group, fmt, nargs = int(m.group(1)), GROUPS.index(m.group(2)), m.group(3), int(m.group(4))
        table[sf_id | group << 12 | nargs << 16 | 0x70000000] = fmt.replace("\\n", "")
    return table


def render(fmt, args):
    fmt = re.sub(r"%(\d*)\.\d+", r"%\1", fmt).replace("%l", "%").replace("%u", "%d").replace("%i", "%d")
    try:
        return fmt % tuple(args)
    except (TypeError, ValueError):
        return fmt + " " + " ".join("%08x" % a for a in args)


def main():
    table = load(sys.argv[1])
    words = [int(w, 16) for w in re.findall(r"\b[0-9a-f]{8}\b", sys.stdin.read())]
    i = 0
    while i < len(words):
        w = words[i]
        if (w & 0xFFF00000) != 0x70000000:
            i += 1
            continue
        nargs = (w >> 16) & 0xF
        ts = words[i + 1] << 32 | words[i + 2] if i + 2 < len(words) else 0
        args = words[i + 3:i + 3 + nargs]
        print("%12d %-4s %s" % (ts, GROUPS[(w >> 12) & 0xF], render(table.get(w, "unknown 0x%08x" % w), args)))
        i += 3 + nargs


main()
