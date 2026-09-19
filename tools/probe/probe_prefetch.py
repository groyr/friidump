#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
キャッシュ確定手法の比較（GCC-4240N / Initio）

各方式で 64 ブロックを連続処理し、DIC gc2.iso と一致するか（完全性）と
所要時間（速度）を同時に測る。READ はブロック先頭(16セクタ境界)で発行する。

方式:
  read2         : READ; READ(data)
  read+prefetch : READ(data); PREFETCH
  prefetch+read : PREFETCH; READ(data)
  flush+read    : FLUSH(READ12 len0); READ(data)
  dummy+read    : READ(遠方,dataなし); READ(data)   ← method4 のダミー
"""

import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
START_BLK = 256
NBLK = 64


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


def read12(s, lba, n, want_data=False):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048 if want_data else 0, want_data, 30)


def prefetch(s, lba, n=16):
    c = bytearray(12)
    c[0] = 0x34
    c[2:6] = lba.to_bytes(4, "big")
    c[7:10] = n.to_bytes(3, "big")
    return s.send(bytes(c), 0, True, 10)


def flush(s, lba):
    c = bytearray(12)
    c[0] = 0xA8
    c[1] = 0x08
    c[2:6] = lba.to_bytes(4, "big")
    return s.send(bytes(c), 0, True, 10)


def prefixes(s):
    return [s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)[2][:6] for k in range(16)]


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

    def body(rd_sec, blk):
        sd = seed_of(blk)
        if sd is None:
            return None
        Cg = lfsr(sd, 2048)
        return bytes(rd_sec[i] ^ C80[i] ^ Cg[i] for i in range(2042))

    def check(rd, blk, ps):
        for k in range(16):
            b = body(rd[k * 2048:(k + 1) * 2048], blk)
            if b is None or ps[k] + b != iso[(blk * 16 + k) * 2048:(blk * 16 + k + 1) * 2048]:
                return False
        return True

    def v_read2(s, lba):
        read12(s, lba, 16)
        return read12(s, lba, 16, want_data=True)[2]

    def v_read_prefetch(s, lba):
        rd = read12(s, lba, 16, want_data=True)[2]
        prefetch(s, lba)
        return rd

    def v_prefetch_read(s, lba):
        prefetch(s, lba)
        return read12(s, lba, 16, want_data=True)[2]

    def v_flush_read(s, lba):
        flush(s, lba)
        return read12(s, lba, 16, want_data=True)[2]

    def v_dummy_read(s, lba):
        read12(s, lba - 512, 16)
        return read12(s, lba, 16, want_data=True)[2]

    variants = {
        "read2": v_read2,
        "read+prefetch": v_read_prefetch,
        "prefetch+read": v_prefetch_read,
        "flush+read": v_flush_read,
        "dummy+read": v_dummy_read,
    }

    s = P.Spti("D")
    try:
        read12(s, (START_BLK + 0x100) * 16, 16)
        ts = []
        for _ in range(12):
            t0 = time.perf_counter()
            prefetch(s, (START_BLK + 0x100) * 16)
            ts.append(time.perf_counter() - t0)
        ts.sort()
        print("PREFETCH 単体: median=%.4fs min=%.4fs max=%.4fs" % (ts[len(ts) // 2], ts[0], ts[-1]))

        for name, fn in variants.items():
            fn(s, START_BLK * 16)
            t0 = time.perf_counter()
            good = 0
            for i in range(NBLK):
                blk = START_BLK + i
                lba = blk * 16
                rd = fn(s, lba)
                ps = prefixes(s)
                if check(rd, blk, ps):
                    good += 1
            dt = time.perf_counter() - t0
            mb = NBLK * 33024 / 1024 / 1024
            print("  %-14s %.3fs  %.2f MB/s  %.1f ms/block  一致 %d/%d"
                  % (name, dt, mb / dt, dt / NBLK * 1000, good, NBLK))
    finally:
        s.close()


if __name__ == "__main__":
    main()
