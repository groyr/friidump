#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# 高速方式プロトタイプ: streaming READ + 先頭6B×16 の E7 で完全な ISO セクタを復元し DIC iso と比較
import spti_probe as P

RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
BASE = 0xA13000
C80 = None


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
    global C80
    C80 = lfsr(0x80, 2048)
    raw = open(RAW, "rb").read()
    iso = open(ISO, "rb").read()

    # 検証用に DIC raw から各ブロックの seed を算出（本来は校正フェーズで求める）
    def seed_of(blk):
        r = raw[blk * 16 * 2064:blk * 16 * 2064 + 2064]
        p = iso[blk * 16 * 2048:blk * 16 * 2048 + 2048]
        c = bytes(a ^ b for a, b in zip(r[12:2060], p[6:2048]))
        for s in range(0x8000):
            if lfsr(s, 8) == c[:8]:
                return s
        return None

    seeds = {}
    for blk in range(0, 64):
        seeds[blk] = seed_of(blk)

    s = P.Spti("D")
    ok_blocks = 0
    for blk in range(0, 64):
        seed = seeds[blk]
        if seed is None:
            print("block %d seed unknown" % blk); continue
        lba = blk * 16
        st, se, rd, dt, ok = s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
        prefixes = []
        for k in range(16):
            st2, se2, d2, dt2, ok2 = s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)
            prefixes.append(d2[:6])
        Cg = lfsr(seed, 2048)
        good = True
        for k in range(16):
            off = k * 2048
            rdk = rd[off:off + 2048]
            body = bytes(rdk[i] ^ C80[i] ^ Cg[i] for i in range(2042))
            Psec = prefixes[k] + body
            if Psec != iso[(lba + k) * 2048:(lba + k) * 2048 + 2048]:
                good = False
        if good:
            ok_blocks += 1
        else:
            print("block %d MISMATCH" % blk)
    print("blocks OK: %d/64" % ok_blocks)
    s.close()


if __name__ == "__main__":
    main()
