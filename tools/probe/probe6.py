#!/usr/bin/env python3
# -*- coding: utf-8 -*-
RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"


def lfsr_bytes(seed, n):
    L = seed & 0x7FFF
    out = bytearray(n)
    for i in range(n):
        b = 0
        for _ in range(8):
            ret = (L >> 14) & 1
            nb = ret ^ ((L >> 10) & 1)
            L = ((L << 1) | nb) & 0x7FFF
            b = (b << 1) | ret
        out[i] = b
    return bytes(out)


CACHE = {}


def cipher(seed, n=2048):
    if (seed, n) not in CACHE:
        CACHE[(seed, n)] = lfsr_bytes(seed, n)
    return CACHE[(seed, n)]


def find_seed(c):
    for s in range(0x8000):
        if cipher(s, 8) == c[:8]:
            return s
    return None


def main():
    raw = open(RAW, "rb").read()
    iso = open(ISO, "rb").read()
    for base in (0, 256, 1024, 65536):
        out = []
        for k in range(16):
            lba = base * 16 + k
            r = raw[lba * 2064:lba * 2064 + 2064]
            p = iso[lba * 2048:lba * 2048 + 2048]
            c = bytes(a ^ b for a, b in zip(r[12:2060], p))
            s = find_seed(c)
            out.append("%X" % s if s is not None else "-")
        print("block %6d (LBA %d..%d): %s" % (base, base * 16, base * 16 + 15, " ".join(out)))


if __name__ == "__main__":
    main()
