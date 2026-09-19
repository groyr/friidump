#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
GCでのREADサイズ掃引（H2の示唆を検証）

通常DVDで 16→24セクタが同一時間だった。GCでも READ(16→24/26) がフラットなら
READサイズ拡大で高速化できる。時間と「実際に復元できるセクタ数」を測る。

GCディスクを挿入してから実行すること。
"""
import statistics
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
CAL_BLK = 16


def r12(s, lba, n):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def main():
    iso = open(ISO, "rb").read()

    def iso_sec(sec):
        return iso[sec * 2048:(sec + 1) * 2048]

    s = P.Spti("D")
    try:
        # 校正
        corr = [[0] * 2042 for _ in range(16)]
        for m in range(16):
            lba = (CAL_BLK + m) * 16
            r12(s, lba, 16)
            rd = r12(s, lba, 16)[2]
            p = iso_sec(lba)
            for i in range(2042):
                corr[m][i] = rd[i] ^ p[6 + i]
        print("校正完了")

        def recover_count(rd, lba, n):
            good = 0
            for k in range(n):
                sec = lba + k
                m = (sec // 16) % 16
                pre = s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)[2][:6]
                body = bytes(rd[k * 2048 + i] ^ corr[m][i] for i in range(2042))
                if pre + body == iso_sec(sec):
                    good += 1
            return good

        for base in (0x1000, 0x80000):
            print("\nlba=0x%06X:" % base)
            for n in (8, 16, 20, 24, 26, 32):
                ts = []
                good = 0
                for _ in range(3):
                    # 窓を遠方へ移す（対象は確実に窓外）
                    r12(s, base + 0x10000, 16)
                    t0 = time.perf_counter()
                    rd = r12(s, base, n)[2]
                    ts.append(time.perf_counter() - t0)
                    good = recover_count(rd, base, n)
                ts.sort()
                t = ts[1]
                print("  READ(%2d): %.4fs  復元 %2d/%2d  %.0f KB/s(生)"
                      % (n, t, good, n, n * 2048 / 1024 / t))
    finally:
        s.close()


if __name__ == "__main__":
    main()
