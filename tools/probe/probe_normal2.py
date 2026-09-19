#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
通常DVDでブリッジの実効上限を測る:
- SET CD SPEED の効果
- READサイズ掃引（固定オーバーヘッドとスループットの分離）
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P


def r12(s, lba, n):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def set_speed(s, speed):
    c = bytearray(12)
    c[0] = 0xBB
    c[2] = (speed >> 8) & 0xFF
    c[3] = speed & 0xFF
    return s.send(bytes(c), 0, True, 10)


def settle(s, lba, n):
    r12(s, lba + 0x20000, 16)
    r12(s, lba, n)
    t0 = time.perf_counter()
    r12(s, lba, n)
    return time.perf_counter() - t0


def main():
    s = P.Spti("D")
    try:
        print("READサイズ掃引 (lba=0x10000, median of 3):")
        for n in (8, 16, 24, 32, 40, 48, 56, 63):
            ts = sorted(settle(s, 0x10000, n) for _ in range(3))
            t = ts[1]
            print("  READ(%2d): %.4fs  %.0f KB/s" % (n, t, n * 2048 / 1024 / t))

        print("\nSET CD SPEED → READ(16) (lba=0x10000):")
        for sp in (0, 176, 706, 1411, 2822, 0xFFFF):
            st, se, d, dt, ok = set_speed(s, sp)
            ts = sorted(settle(s, 0x10000, 16) for _ in range(3))
            print("  speed=%5d st=0x%02X  median=%.4fs" % (sp, st, ts[1]))
    finally:
        s.close()


if __name__ == "__main__":
    main()
