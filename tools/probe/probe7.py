#!/usr/bin/env python3
# -*- coding: utf-8 -*-
RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"


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


raw = open(RAW, "rb").read()
iso = open(ISO, "rb").read()

# LBA1 で LFSR 実装検証
r1 = raw[1 * 2064:2 * 2064]
c1 = bytes(a ^ b for a, b in zip(r1[12:2060], iso[1 * 2048:2 * 2048]))
print("c(LBA1)[:8] =", c1[:8].hex(), " LFSR(0x180)[:8] =", lfsr(0x180, 8).hex(), " eq=", c1 == lfsr(0x180, 2048))

# 他ブロックの c と、オフセットをずらした LFSR 一致探索（先頭4バイト）
for lba in (0, 0x1000, 0x1001, 0x2000, 0x40000):
    r = raw[lba * 2064:lba * 2064 + 2064]
    c = bytes(a ^ b for a, b in zip(r[12:2060], iso[lba * 2048:lba * 2048 + 2048]))
    best = None
    for s in range(0x8000):
        cs = lfsr(s, 72)
        for off in range(0, 64):
            if cs[off:off + 4] == c[:4]:
                best = (s, off)
                break
        if best:
            break
    print("LBA=0x%-6X c[:8]=%s  seed@off=%s" % (lba, c[:8].hex(), best))
