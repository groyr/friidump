#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""READ サイクルのコマンド内訳計測（ハング回避のため PREFETCH は含めない）"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
START_BLK = 256
NBLK = 32


def read12(s, lba, n, want_data=False):
    # ゼロ長転送はこのブリッジで SPTI タイムアウトするため、常に実バッファを渡す
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def flush(s, lba):
    c = bytearray(12)
    c[0] = 0xA8
    c[1] = 0x08
    c[2:6] = lba.to_bytes(4, "big")
    return s.send(bytes(c), 0, True, 10)


def prefix_time(s):
    t = 0.0
    for k in range(16):
        t0 = time.perf_counter()
        s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)
        t += time.perf_counter() - t0
    return t


def bench(s, name, fn):
    read12(s, (START_BLK + 0x200) * 16, 16, True)
    t_total = 0.0
    parts = None
    for i in range(NBLK):
        blk = START_BLK + i
        lba = blk * 16
        t0 = time.perf_counter()
        ts = fn(s, lba)
        pe = prefix_time(s)
        t_total += time.perf_counter() - t0
        ts = ts + [pe]
        parts = ts if parts is None else [a + b for a, b in zip(parts, ts)]
    parts = [x / NBLK * 1000 for x in parts]
    print("  %-16s total %.1f ms/block   内訳(ms): %s" % (name, t_total / NBLK * 1000,
          ", ".join("%.1f" % x for x in parts)))


def main():
    s = P.Spti("D")
    try:
        bench(s, "read x2", lambda s, l:
              [time_cmd(s, lambda: read12(s, l, 16, False)),
               time_cmd(s, lambda: read12(s, l, 16, True))])
        bench(s, "dummy + read", lambda s, l:
              [time_cmd(s, lambda: read12(s, l - 512, 16, False)),
               time_cmd(s, lambda: read12(s, l, 16, True))])
    finally:
        s.close()


def time_cmd(s, fn):
    t0 = time.perf_counter()
    fn()
    return time.perf_counter() - t0


if __name__ == "__main__":
    main()
