#!/usr/bin/env python3
# usage: fwif-dwarf.py <llvm-dwarfdump --debug-info output> [-dN] STRUCT...
import re
import sys

die_re = re.compile(r"^(0x[0-9a-f]+):(\s+)(DW_TAG_\w+|NULL)")
attr_re = re.compile(r"^\s+(DW_AT_\w+)\s+\((.*)\)$")


def parse(path):
    dies = {}
    stack = []
    cur = None
    for line in open(path, errors="replace"):
        m = die_re.match(line)
        if m:
            off, indent, tag = int(m.group(1), 16), len(m.group(2)), m.group(3)
            while stack and stack[-1][0] >= indent:
                stack.pop()
            if tag == "NULL":
                cur = None
                continue
            cur = {"off": off, "tag": tag, "attrs": {}, "children": []}
            dies[off] = cur
            if stack:
                stack[-1][1]["children"].append(cur)
            stack.append((indent, cur))
            continue
        m = attr_re.match(line)
        if m and cur is not None:
            cur["attrs"][m.group(1)] = m.group(2)
    return dies


def num(v):
    m = re.match(r"(0x[0-9a-f]+|\d+)", v)
    return int(m.group(1), 0) if m else None


def name(d):
    n = d["attrs"].get("DW_AT_name")
    return n.strip('"') if n else None


def ref(d):
    t = d["attrs"].get("DW_AT_type")
    return num(t) if t else None


def resolve(dies, off):
    d = dies[off]
    while d["tag"] in ("DW_TAG_typedef", "DW_TAG_volatile_type", "DW_TAG_const_type"):
        d = dies[ref(d)]
    return d


def count(c):
    if "DW_AT_count" in c["attrs"]:
        return num(c["attrs"]["DW_AT_count"])
    return num(c["attrs"]["DW_AT_upper_bound"]) + 1


def size(dies, off):
    d = resolve(dies, off)
    if "DW_AT_byte_size" in d["attrs"]:
        return num(d["attrs"]["DW_AT_byte_size"])
    if d["tag"] == "DW_TAG_array_type":
        n = 1
        for c in d["children"]:
            if c["tag"] == "DW_TAG_subrange_type":
                n *= count(c)
        return n * size(dies, ref(d))
    if d["tag"] == "DW_TAG_pointer_type":
        return 4
    return None


def tname(dies, off):
    d = dies[off]
    if name(d):
        return name(d)
    if d["tag"] == "DW_TAG_array_type":
        dims = "".join("[%d]" % count(c) for c in d["children"] if c["tag"] == "DW_TAG_subrange_type")
        return tname(dies, ref(d)) + dims
    if d["tag"] == "DW_TAG_pointer_type":
        return "ptr"
    if d["tag"] in ("DW_TAG_volatile_type", "DW_TAG_const_type"):
        return tname(dies, ref(d))
    return d["tag"]


def dump(dies, d, base=0, prefix="", depth=0, maxdepth=1):
    for c in d["children"]:
        if c["tag"] != "DW_TAG_member":
            continue
        off = base + (num(c["attrs"].get("DW_AT_data_member_location", "0")) or 0)
        t = ref(c)
        print("%5d 0x%04x %5s  %s%s : %s" % (off, off, size(dies, t), prefix, name(c), tname(dies, t)))
        r = resolve(dies, t)
        if depth + 1 < maxdepth and r["tag"] in ("DW_TAG_structure_type", "DW_TAG_union_type"):
            dump(dies, r, off, prefix + name(c) + ".", depth + 1, maxdepth)


def main():
    dies = parse(sys.argv[1])
    depth = 1
    for want in sys.argv[2:]:
        if want.startswith("-d"):
            depth = int(want[2:])
            continue
        for d in dies.values():
            if d["tag"] in ("DW_TAG_structure_type", "DW_TAG_typedef", "DW_TAG_union_type") and name(d) == want:
                r = resolve(dies, d["off"])
                if r["tag"] == "DW_TAG_structure_type" and r["children"]:
                    print("%s: size %s" % (want, size(dies, r["off"])))
                    dump(dies, r, maxdepth=depth)
                    break


main()
