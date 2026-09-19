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


for lba in (1, 0x1000):
    r = raw[lba * 2064:lba * 2064 + 2064]
    p = iso[lba * 2048:lba * 2048 + 2048]
    c = bytes(a ^ b for a, b in zip(r[12:2060], p))
    hits = []
    for s in range(0x10000):
        cs = lfsr(s, 8)
        if cs == c[:8] or lfsr(((s >> 8) | (s << 8)) & 0xFFFF, 8) == c[:8]:
            hits.append(s)
    print("LBA=0x%X c[:8]=%s hits=%s" % (lba, c[:8].hex(), ["0x%04X" % h for h in hits[:8]]))
