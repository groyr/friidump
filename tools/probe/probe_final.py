#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
最終比較: READ確定方法ごとの 速度 と 完全性（gc2.iso と照合）

いずれも READ はブロック先頭(16境界)、E7 6B×16 で前方6Bを取得。
  read2   : READ; READ
  read    : READ のみ
  sleep   : READ; sleep(t); E7
  prefetch: READ; PREFETCH; E7
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
START_BLK = 256
NBLK = 48


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


def r12(s, lba, n):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)[2]


def pf(s, lba):
    c = bytearray(12)
    c[0] = 0x34
    c[2:6] = lba.to_bytes(4, "big")
    c[7:10] = (16).to_bytes(3, "big")
    return s.send(bytes(c), 0, True, 10)


def main():
    raw = open(RAW, "rb").read()
    iso = open(ISO, "rb").read()
    first8 = {lfsr(s, 8): s for s in range(0x8000)}

    def Cg(blk):
        r = raw[blk * 16 * 2064:blk * 16 * 2064 + 2064]
        p = iso[blk * 16 * 2048:blk * 16 * 2048 + 2048]
        c = bytes(a ^ b for a, b in zip(r[12:2060], p[6:2048]))
        sd = first8.get(c[:8])
        return lfsr(sd, 2048) if sd is not None else None

    C80 = lfsr(0x80, 2048)
    cgcache = {}

    def cg(blk):
        if blk not in cgcache:
            cgcache[blk] = Cg(blk)
        return cgcache[blk]

    def verify(s, rd, blk):
        for k in range(16):
            pre = s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)[2][:6]
            C = cg(blk)
            if C is None:
                return False
            body = bytes(rd[k * 2048 + i] ^ C80[i] ^ C[i] for i in range(2042))
            if pre + body != iso[(blk * 16 + k) * 2048:(blk * 16 + k + 1) * 2048]:
                return False
        return True

    # 参照確認: read2 で blocks 256..259 が gc2 と一致するか
    s = P.Spti("D")
    try:
        r12(s, (START_BLK + 8) * 16, 16)
        ref_ok = 0
        for i in range(4):
            blk = START_BLK + i
            r12(s, blk * 16, 16)
            rd = r12(s, blk * 16, 16)
            if verify(s, rd, blk):
                ref_ok += 1
        print("参照確認(read2, blocks %d..%d): %d/4" % (START_BLK, START_BLK + 3, ref_ok))

        def run(name, fn):
            r12(s, (START_BLK + 15) * 16, 16)
            t0 = time.perf_counter()
            good = 0
            for i in range(NBLK):
                blk = START_BLK + i
                lba = blk * 16
                rd = fn(s, lba)
                if verify(s, rd, blk):
                    good += 1
            dt = time.perf_counter() - t0
            print("  %-10s %.3fs  %.2f MB/s  %.1f ms/block  一致 %d/%d"
                  % (name, dt, NBLK * 33024 / 1048576 / dt, dt / NBLK * 1000, good, NBLK))

        args = int(sys.argv[1]) if len(sys.argv) > 1 else 0
        run("read2", lambda s, l: (r12(s, l, 16), r12(s, l, 16))[1])
        run("read", lambda s, l: r12(s, l, 16))
        for t in (0.01, 0.03, 0.06):
            run("sleep%02d" % int(t * 1000), lambda s, l, t=t: (r12(s, l, 16), time.sleep(t), r12(s, l, 16))[2])
        run("prefetch", lambda s, l: (r12(s, l, 16), pf(s, l), r12(s, l, 16))[2])
    finally:
        s.close()


if __name__ == "__main__":
    main()
