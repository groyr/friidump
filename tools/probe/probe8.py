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

# 全 seed の長いストリーム (先頭 2048*16 + 64) を用意
STREAM = {s: lfsr(s, 2048 * 16 + 64) for s in range(0x8000)}


def find(c, phase_max=2048 * 16):
    for s, st in STREAM.items():
        for off in range(0, phase_max, 1):
            if st[off:off + 4] == c[:4]:
                return s, off
    return None


for base in (0, 256):
    print("== block %d ==" % base)
    for k in range(16):
        lba = base * 16 + k
        r = raw[lba * 2064:lba * 2064 + 2064]
        c = bytes(a ^ b for a, b in zip(r[12:2060], iso[lba * 2048:lba * 2048 + 2048]))
        res = find(c)
        print("  LBA %7d c[:8]=%s -> %s" % (lba, c[:8].hex(), ("seed=0x%04X off=%d" % res) if res else None))
