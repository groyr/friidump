#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# raw と iso から GC のスクランブル seed を割り出せるか確認
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


def main():
    raw = open(RAW, "rb").read()
    iso = open(ISO, "rb").read()
    for lba in (0, 1, 0x1000, 0x1001, 0x40000):
        r = raw[lba * 2064:lba * 2064 + 2064]
        p = iso[lba * 2048:lba * 2048 + 2048]
        for name, c in (("raw12^iso", bytes(a ^ b for a, b in zip(r[12:2060], p))),
                        ("raw6^iso", bytes(a ^ b for a, b in zip(r[6:2054], p)))):
            seed = None
            for s in range(0x8000):
                if lfsr_bytes(s, 8) == c[:8]:
                    seed = s
                    break
            full = (lfsr_bytes(seed, 2048) == c) if seed is not None else False
            print("LBA=0x%X %s seed=%s full=%s" % (lba, name, ("0x%04X" % seed) if seed is not None else None, full))


if __name__ == "__main__":
    main()
