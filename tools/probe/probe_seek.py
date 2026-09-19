#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
SEEK(10) 方式の検証: SEEK(lba) → READ(lba) → E7 6B×16

baseline(READ×2) と比較し、速度と完全性（corr校正で gc2.iso と照合）を測る。
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
CAL_BLK = 16
START_BLK = 256
NBLK = 64


def r12(s, lba, n=16):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def seek(s, lba):
    c = bytearray(12)
    c[0] = 0x2B
    c[2:6] = lba.to_bytes(4, "big")
    return s.send(bytes(c), 0, True, 10)


def main():
    iso = open(ISO, "rb").read()

    def iso_sec(sec):
        return iso[sec * 2048:(sec + 1) * 2048]

    s = P.Spti("D")
    try:
        corr = [[0] * 2042 for _ in range(16)]
        for m in range(16):
            blk = CAL_BLK + m
            lba = blk * 16
            r12(s, lba)
            rd = r12(s, lba)[2]
            p = iso_sec(lba)
            for i in range(2042):
                corr[m][i] = rd[i] ^ p[6 + i]
        print("校正完了")

        def e7_prefixes(s, n=16):
            return [s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)[2][:6]
                    for k in range(n)]

        def recover(rd, sec, cache_base):
            off = sec - cache_base
            if off < 0 or off > 25:
                return None
            pre = s.send(P.e7_cdb(BASE + off * 2064 + 6, 6), 6, True, 10)[2][:6]
            C = corr[(sec // 16) % 16]
            return pre + bytes(rd[i] ^ C[i] for i in range(2042))

        def verify_block(s, rd, blk):
            for k in range(16):
                if recover(rd[k * 2048:(k + 1) * 2048], blk * 16 + k, blk * 16) != iso_sec(blk * 16 + k):
                    return False
            return True

        def run(name, fn):
            r12(s, (START_BLK + 20) * 16)  # プライム
            t0 = time.perf_counter()
            good = 0
            for i in range(NBLK):
                blk = START_BLK + i
                rd = fn(s, blk * 16)
                if verify_block(s, rd, blk):
                    good += 1
            dt = time.perf_counter() - t0
            print("  %-14s %.3fs  %.2f MB/s  %.1f ms/block  一致 %d/%d"
                  % (name, dt, NBLK * 33024 / 1048576 / dt, dt / NBLK * 1000, good, NBLK))

        def v_read2(s, l):
            r12(s, l)
            return r12(s, l)[2]

        def v_seek_read(s, l):
            seek(s, l)
            return r12(s, l)[2]

        run("read2", v_read2)
        run("seek+read", v_seek_read)
    finally:
        s.close()


if __name__ == "__main__":
    main()
