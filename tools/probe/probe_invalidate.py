#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
S3: 窓移動を安価なコマンドで代替できるか

現行: READ(no-op 13ms) → READ(settle 80ms) → E7
候補: SYNC/SEEK(〜0ms)  → READ(settle) → E7  で 13ms を削減できるか
"""
import statistics
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
A = 0x1000


def r12(s, lba, n=16):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def sync_cache(s, lba, n=16):
    c = bytearray(12)
    c[0] = 0x35
    c[2:6] = lba.to_bytes(4, "big")
    c[7:10] = n.to_bytes(3, "big")
    return s.send(bytes(c), 0, True, 10)


def seek(s, lba):
    c = bytearray(12)
    c[0] = 0x2B
    c[2:6] = lba.to_bytes(4, "big")
    return s.send(bytes(c), 0, True, 10)


def cache_sn(s):
    st, se, d, dt, ok = s.send(P.e7_cdb(BASE, 12), 12, True, 10)
    return P.sector_num(d)


def timeit(fn):
    t0 = time.perf_counter()
    r = fn()
    return time.perf_counter() - t0, r


def main():
    s = P.Spti("D")
    try:
        # 候補コマンド単体のレイテンシ（Bの窓内で無害に発行）
        for name, fn in (("SYNC_CACHE", lambda: sync_cache(s, A + 16)),
                         ("SEEK(10)", lambda: seek(s, A + 16))):
            ts = []
            for _ in range(5):
                dt, r = timeit(fn)
                ts.append(dt)
            st = r[0]
            print("S3 %-10s latency median=%.4fs status=0x%02X" % (name, statistics.median(ts), st))

        def baseline(s):
            r12(s, A)                 # 窓=A
            t_trig, _ = timeit(lambda: r12(s, A + 16))   # trigger(no-op)
            t_set, _ = timeit(lambda: r12(s, A + 16))    # settle
            return t_trig, t_set

        def variant(s, pre):
            r12(s, A)
            t_pre, r = timeit(lambda: pre(s, A + 16))
            t_rd, _ = timeit(lambda: r12(s, A + 16))
            sn = cache_sn(s)
            return t_pre, t_rd, sn

        tr, ts = baseline(s)
        print("\nbaseline: trigger=%.4fs settle=%.4fs total=%.4fs (cache_sn=0x%X)"
              % (tr, ts, tr + ts, cache_sn(s)))

        for name, pre in (("SYNC_CACHE", sync_cache), ("SEEK(10)", seek)):
            tp, trd, sn = variant(s, pre)
            print("%-10s: pre=%.4fs read=%.4fs total=%.4fs cache_sn=0x%X (期待 0x%X)"
                  % (name, tp, trd, tp + trd, sn, A + 16 + 0x30000))
    finally:
        s.close()


if __name__ == "__main__":
    main()
