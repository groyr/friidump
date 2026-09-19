#!/usr/bin/env python3
# -*- coding: utf-8 -*-
RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"

raw = open(RAW, "rb").read()
iso = open(ISO, "rb").read()


def lfsr(seed, n):
    L = seed & 0x7FFF
    o = bytearray(n)
    for i in range(n):
        b = 0
        for _ in range(8):
            r = (L >> 14) & 1
            L = ((L << 1) | (r ^ ((L >> 10) & 1))) & 0x7FFF
            b = (b << 1) | r
        o[i] = b
    return bytes(o)


OFFS = (0, 6, 8, 16, 100)


def scan(c):
    res = {o: None for o in OFFS}
    need = max(OFFS) + 6
    for s in range(0x8000):
        st = lfsr(s, need)
        for off in OFFS:
            if res[off] is None and st[off:off + 6] == c[off:off + 6]:
                res[off] = s
        if all(v is not None for v in res.values()):
            break
    return res


for lba in (0x1000, 0x1001):
    r = raw[lba * 2064:lba * 2064 + 2064]
    p = iso[lba * 2048:lba * 2048 + 2048]
    c = bytes(a ^ b for a, b in zip(r[12:2060], p))
    print("LBA=0x%X %s" % (lba, scan(c)))
