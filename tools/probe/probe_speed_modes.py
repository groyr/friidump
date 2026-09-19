#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
S1/S2b/S5: 速度・モード掃引とE7最適化（GCディスクのままで実施可能）

- ベースライン settle READ 時間
- SET CD SPEED (0xBB) 各値
- SET STREAMING (0xB6) 各値
- READ(10) vs READ(12)、FUA 有無
- E7 6B×16 vs 33024×1
"""
import statistics
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
LBA = 0x1000


def r12(s, lba, n, byte1=0x00):
    return s.send(P.read12_cdb(lba, n, byte1, streaming=True), n * 2048, True, 30)


def r10(s, lba, n, byte1=0x00):
    c = bytearray(12)
    c[0] = 0x28
    c[1] = byte1
    c[2] = (lba >> 24) & 0xFF
    c[3] = (lba >> 16) & 0xFF
    c[4] = (lba >> 8) & 0xFF
    c[5] = lba & 0xFF
    c[7] = (n >> 8) & 0xFF
    c[8] = n & 0xFF
    return s.send(bytes(c), n * 2048, True, 30)


def set_cd_speed(s, speed):
    c = bytearray(12)
    c[0] = 0xBB
    c[2] = (speed >> 8) & 0xFF
    c[3] = speed & 0xFF
    return s.send(bytes(c), 0, True, 10)


def set_streaming(s, speed):
    c = bytearray(12)
    c[0] = 0xB6
    c[10] = 28
    b = bytearray(28)
    b[12:16] = speed.to_bytes(4, "big")
    b[16:20] = (1000).to_bytes(4, "big")
    b[20:24] = speed.to_bytes(4, "big")
    b[24:28] = (1000).to_bytes(4, "big")
    return s.send(bytes(c), 0, True, 10)


def settle_times(s, readfn, n=4):
    ts = []
    for i in range(n):
        r12(s, LBA + 0x4000 + i * 16, 16)  # 窓を飛ばす
        t0 = time.perf_counter()
        readfn(s, LBA, 16)
        ts.append(time.perf_counter() - t0)
    ts.sort()
    return statistics.median(ts), ts[0]


def main():
    s = P.Spti("D")
    try:
        auto = settle_times(s, lambda s, l, n: r12(s, l, n))
        print("S1 ベースライン settle READ: median=%.4fs min=%.4fs" % auto)

        print("\nS2b SET CD SPEED (0xBB) → settle READ:")
        for sp in (0, 176, 353, 706, 1058, 1411, 2116, 2822, 5644, 0xFFFF):
            st, se, d, dt, ok = set_cd_speed(s, sp)
            med, mn = settle_times(s, lambda s, l, n: r12(s, l, n))
            print("  speed=%5d status=0x%02X sense=%s  median=%.4fs min=%.4fs"
                  % (sp, st, se[:3].hex(), med, mn))
        set_cd_speed(s, 0)

        print("\nS2b SET STREAMING (0xB6) → settle READ:")
        for sp in (0, 176, 706, 1411, 2822, 5644, 0xFFFF):
            st, se, d, dt, ok = set_streaming(s, sp)
            med, mn = settle_times(s, lambda s, l, n: r12(s, l, n))
            print("  speed=%5d status=0x%02X sense=%s  median=%.4fs min=%.4fs"
                  % (sp, st, se[:3].hex(), med, mn))

        print("\nS2b READ(10) vs READ(12), FUA:")
        med10, mn10 = settle_times(s, lambda s, l, n: r10(s, l, n))
        print("  READ(10) streaming          median=%.4fs min=%.4fs" % (med10, mn10))
        med12f, mn12f = settle_times(s, lambda s, l, n: r12(s, l, n, byte1=0x08))
        print("  READ(12) streaming+FUA(0x08) median=%.4fs min=%.4fs" % (med12f, mn12f))

        print("\nS5 E7 6B×16 vs 33024×1:")
        r12(s, LBA, 16)
        t0 = time.perf_counter()
        for k in range(16):
            s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)
        t16 = time.perf_counter() - t0
        t0 = time.perf_counter()
        s.send(P.e7_cdb(BASE, 33024), 33024, True, 10)
        t1 = time.perf_counter() - t0
        print("  16x6B=%.4fs  33024x1=%.4fs" % (t16, t1))
    finally:
        s.close()


if __name__ == "__main__":
    main()
