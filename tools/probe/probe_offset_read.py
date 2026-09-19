#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
READ の要求 LBA とホストデータ/キャッシュの整列関係を、DIC 参照(gc2)との照合で特定する。

狙い:
  READ を「ブロック先頭+10」で発行すると、キャッシュ窓が前回と重ならず 1 回で確定する。
  そのときホストデータ rd が「ブロック先頭に整列ダウン」しているかを確かめる。
"""

import sys

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
START = 0x1000
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


def main():
    raw = open(RAW, "rb").read()
    iso = open(ISO, "rb").read()

    # seed -> 先頭8バイト暗号 の逆引きテーブル
    first8 = {}
    for s in range(0x8000):
        first8[lfsr(s, 8)] = s

    def seed_of(blk):
        r = raw[blk * 16 * 2064:blk * 16 * 2064 + 2064]
        p = iso[blk * 16 * 2048:blk * 16 * 2048 + 2048]
        c = bytes(a ^ b for a, b in zip(r[12:2060], p[6:2048]))
        return first8.get(c[:8])

    C80 = lfsr(0x80, 2048)

    def body_of(rd_sec, blk):
        """ホストデータ1セクタ(2048B)から、先頭6Bを除く 2042B を復号して返す。"""
        seed = seed_of(blk)
        if seed is None:
            return None
        Cg = lfsr(seed, 2048)
        return bytes(rd_sec[i] ^ C80[i] ^ Cg[i] for i in range(2042))

    def match_count(rd, base_sector, blk_of):
        n = 0
        for k in range(16):
            sec = base_sector + k
            body = body_of(rd[k * 2048:(k + 1) * 2048], blk_of(sec))
            if body is not None and body == iso[sec * 2048 + 6:(sec + 1) * 2048]:
                n += 1
        return n

    s = P.Spti("D")
    try:
        B = START // 16
        print("block %d (lba 0x%X), seed=%s" % (B, B * 16, hex(seed_of(B) or -1)))
        for off in (0, 8, 10, 15):
            # リセット（遠方 READ）
            s.send(P.read12_cdb(START + 0x8000, 16, 0, streaming=True), 16 * 2048, True, 30)
            lba = B * 16 + off
            st, se, rd, dt, ok = s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
            st2, se2, d2, dt2, ok2 = s.send(P.e7_cdb(BASE, 12), 12, True, 10)
            cached = P.sector_num(d2)
            # rd を「整列ダウンブロック B 起点」とみなして照合
            m_aligned = match_count(rd, B * 16, lambda sec: B)
            # rd を「要求 LBA 起点」とみなして照合
            m_req = match_count(rd, lba, lambda sec: sec // 16)
            print("  off=%2d: cached_sn=0x%X (期待 block先頭=0x%X)  整列down=%d/16  要求LBA起点=%d/16  %.3fs"
                  % (off, cached, B * 16 + 0x30000, m_aligned, m_req, dt))
    finally:
        s.close()


if __name__ == "__main__":
    main()
