#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# 高速方式の連続ブロック成立率を測定（ブート領域を除く）
import spti_probe as P

RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
BASE = 0xA13000


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
C80 = lfsr(0x80, 2048)


def seed_of(blk):
    r = raw[blk * 16 * 2064:blk * 16 * 2064 + 2064]
    p = iso[blk * 16 * 2048:blk * 16 * 2048 + 2048]
    c = bytes(a ^ b for a, b in zip(r[12:2060], p[6:2048]))
    for s in range(0x8000):
        if lfsr(s, 8) == c[:8]:
            return s
    return None


s = P.Spti("D")
ok = 0
tot = 0
for blk in range(16, 96):
    seed = seed_of(blk)
    if seed is None:
        continue
    lba = blk * 16
    # 二重 READ でキャッシュ更新を確実に
    s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
    st, se, rd, dt, okr = s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
    Cg = lfsr(seed, 2048)
    good = True
    for k in range(16):
        st2, se2, d2, dt2, ok2 = s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)
        off = k * 2048
        body = bytes(rd[off + i] ^ C80[i] ^ Cg[i] for i in range(2042))
        if d2[:6] + body != iso[(lba + k) * 2048:(lba + k) * 2048 + 2048]:
            good = False
    tot += 1
    ok += good
print("fast match blocks: %d/%d" % (ok, tot))
s.close()
