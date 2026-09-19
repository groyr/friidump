#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""method11相当（trigger READ + settle READ + E7×16）の総コストを半径別に測定し、全ディスク時間を見積もる。"""
import statistics
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

DISC_END = 0xAE0B0
BASE = 0xA13000


def r12(s, lba, n=16):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def block_total(s, lba):
    t0 = time.perf_counter()
    r12(s, lba)          # trigger
    r12(s, lba)          # settle
    for k in range(16):  # E7 prefixes
        s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)
    return time.perf_counter() - t0


def main():
    s = P.Spti("D")
    try:
        starts = [(0x1000 + i * 0x10000, "0x%06X" % (0x1000 + i * 0x10000)) for i in range(11)]
        per = {}
        for start, label in starts:
            # ウォームアップ（窓を整える）
            r12(s, start + 0x8000)
            r12(s, start)
            t0 = time.perf_counter()
            n = 32
            for i in range(1, n + 1):
                block_total(s, start + i * 16)
            dt = time.perf_counter() - t0
            per[start] = dt / n
            print("  %s (0x%06X): %.1f ms/block (%.2f MB/s)"
                  % (label, start, dt / n * 1000, 33024 / 1048576 / (dt / n)))
        # 円盤全体を線形補間で概算（内周→外周）
        vals = list(per.values())
        avg = sum(vals) / len(vals)
        print("概算平均 %.1f ms/block → 全ディスク %.1f分" % (avg * 1000, 44555 * avg / 60))
    finally:
        s.close()


if __name__ == "__main__":
    main()
