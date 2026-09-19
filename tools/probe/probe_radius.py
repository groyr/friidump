#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""settle READ時間の半径依存とSEEK併用の分散を確認する。"""
import statistics
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

LBAS = [0x1000, 0x20000, 0x50000, 0x80000, 0xB0000]


def r12(s, lba, n=16):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def seek(s, lba):
    c = bytearray(12)
    c[0] = 0x2B
    c[2:6] = lba.to_bytes(4, "big")
    return s.send(bytes(c), 0, True, 10)


def main():
    s = P.Spti("D")
    try:
        print("LBA別 settle READ (READ×2の2回目, median of 3):")
        for lba in LBAS:
            ts = []
            for i in range(3):
                r12(s, lba + 0x8000 + i * 16)  # 窓を飛ばす
                r12(s, lba)
                t0 = time.perf_counter()
                r12(s, lba)
                ts.append(time.perf_counter() - t0)
            print("  lba=0x%06X median=%.4fs min=%.4fs" % (lba, statistics.median(ts), min(ts)))

        print("\nSEEK+READ の分散 (lba=0x1000, 12回):")
        ts = []
        for i in range(12):
            r12(s, 0x1000 + 0x8000 + i * 16)
            seek(s, 0x1000)
            t0 = time.perf_counter()
            r12(s, 0x1000)
            ts.append(time.perf_counter() - t0)
        ts.sort()
        print("  " + " ".join("%.3f" % t for t in ts))
    finally:
        s.close()


if __name__ == "__main__":
    main()
