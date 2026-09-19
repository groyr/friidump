#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ディスク全体の settle READ 時間プロファイル（CAV/半径依存の把握）

- LBAをスキャンして settle READ 時間を測定
- 内周/外周で READ サイズ 16/32/64 の依存を確認
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

DISC_END = 0xAE0B0


def r12(s, lba, n=16):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def set_cd_speed(s, speed):
    c = bytearray(12)
    c[0] = 0xBB
    c[2] = (speed >> 8) & 0xFF
    c[3] = speed & 0xFF
    return s.send(bytes(c), 0, True, 10)


def settle(s, lba, n=16):
    r12(s, (lba + 0x10000) % DISC_END)  # 窓を飛ばす
    r12(s, lba)
    t0 = time.perf_counter()
    r12(s, lba, n)
    return time.perf_counter() - t0


def main():
    s = P.Spti("D")
    try:
        print("LBAプロファイル (settle READ, READ×2の2回目):")
        lba = 0x1000
        vals = []
        while lba < DISC_END - 0x2000:
            dt = settle(s, lba)
            vals.append((lba, dt))
            lba += 0x8000
        for l, dt in vals[::6]:
            print("  0x%06X  %.4fs" % (l, dt))
        avg = sum(dt for _, dt in vals) / len(vals)
        print("  平均 %.4fs  (n=%d) → 換算 %.1f分 (44555ブロック, E7込)"
              % (avg, len(vals), 44555 * (avg + 0.0116) / 60))

        print("\nサイズ依存 (内周 0x1000 / 外周 0x80000):")
        for base in (0x1000, 0x80000):
            for n in (16, 32, 64):
                ts = []
                for _ in range(3):
                    ts.append(settle(s, base, n))
                ts.sort()
                print("  lba=0x%06X READ(%2d) median=%.4fs" % (base, n, ts[1]))
    finally:
        s.close()


if __name__ == "__main__":
    main()
