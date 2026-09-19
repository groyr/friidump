#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
READ(26) のホストデータが 26 セクタ分（=キャッシュ窓）正しいか、
および非整列 LBA(s+26) でも正しいかを検証する。
正しければ 26 セクタ刻みで 1 READ/26セクタ にできる。
"""
import sys
sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
START = 0x1000


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


def main():
    raw = open(RAW, "rb").read()
    iso = open(ISO, "rb").read()
    first8 = {lfsr(s, 8): s for s in range(0x8000)}

    def seed_of(blk):
        r = raw[blk * 16 * 2064:blk * 16 * 2064 + 2064]
        p = iso[blk * 16 * 2048:blk * 16 * 2048 + 2048]
        c = bytes(a ^ b for a, b in zip(r[12:2060], p[6:2048]))
        return first8.get(c[:8])

    C80 = lfsr(0x80, 2048)
    cache = {}

    def body(rd_sec, blk):
        if blk not in cache:
            sd = seed_of(blk)
            cache[blk] = lfsr(sd, 2048) if sd is not None else None
        Cg = cache[blk]
        if Cg is None:
            return None
        return bytes(rd_sec[i] ^ C80[i] ^ Cg[i] for i in range(2042))

    s = P.Spti("D")
    try:
        for lba in (START, START + 26, START + 16):
            s.send(P.read12_cdb(START + 0x4000, 16, 0, streaming=True), 16 * 2048, True, 30)
            st, se, rd, dt, ok = s.send(P.read12_cdb(lba, 26, 0, streaming=True), 26 * 2048, True, 30)
            match = 0
            bad = []
            for k in range(26):
                sec = lba + k
                b = body(rd[k * 2048:(k + 1) * 2048], sec // 16)
                if b is not None and b == iso[sec * 2048 + 6:(sec + 1) * 2048]:
                    match += 1
                else:
                    bad.append(k)
            print("READ(26) lba=0x%X (blk先頭からのoffset %d): 一致 %d/26  不一致offsets=%s  %.3fs"
                  % (lba, lba - START, match, bad[:8], dt))
    finally:
        s.close()


if __name__ == "__main__":
    main()
