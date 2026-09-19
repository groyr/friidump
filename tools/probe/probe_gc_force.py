#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
GC: 「窓の外へ飛べば単発READで正しいデータが取れるか」を検証。
- fwd32 : 2ブロック刻み(32セクタ)前進で単発READ(16) → 偶数ブロック
- fwd26 : 2ブロック刻みで単発READ(26) → 26セクタ復元
- back  : 遠くへ飛んでから手前を単発READ（後方ジャンプ）
- fwd16 : 比較（+16、デシンクするはず）
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
START_BLK = 256


def r12(s, lba, n):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def main():
    iso = open(ISO, "rb").read()

    def iso_sec(sec):
        return iso[sec * 2048:(sec + 1) * 2048]

    s = P.Spti("D")
    try:
        corr = [[0] * 2042 for _ in range(16)]
        for m in range(16):
            lba = (16 + m) * 16
            r12(s, lba, 16)
            rd = r12(s, lba, 16)[2]
            p = iso_sec(lba)
            for i in range(2042):
                corr[m][i] = rd[i] ^ p[6 + i]
        print("校正完了")

        def body_ok(rd_sec, sec):
            m = (sec // 16) % 16
            body = bytes(rd_sec[i] ^ corr[m][i] for i in range(2042))
            return body == iso_sec(sec)[6:]

        def check(rd, lba, n):
            good = 0
            for k in range(n):
                if body_ok(rd[k * 2048:(k + 1) * 2048], lba + k):
                    good += 1
            return good

        def run(name, seq):
            t0 = time.perf_counter()
            good = 0
            tot = 0
            for lba, n in seq:
                rd = r12(s, lba, n)[2]
                good += check(rd, lba, n)
                tot += n
            dt = time.perf_counter() - t0
            print("%-8s %dセクタ一致 %d/%d  %.2fs  %.1f ms/read"
                  % (name, tot, good, tot, dt, dt / max(1, len(seq)) * 1000))

        N = 32
        # fwd16（デシンク確認）
        for _ in range(3):
            r12(s, (START_BLK - 2) * 16, 16)
        run("fwd16", [(START_BLK * 16 + i * 16, 16) for i in range(N)])
        # fwd32
        for _ in range(3):
            r12(s, (START_BLK - 2) * 16, 16)
        run("fwd32", [(START_BLK * 16 + i * 32, 16) for i in range(N)])
        # fwd26 (2ブロック刻み)
        for _ in range(3):
            r12(s, (START_BLK - 2) * 16, 16)
        run("fwd26", [(START_BLK * 16 + i * 32, 26) for i in range(N)])
        # 後方ジャンプ: まず遠くへ、その後 START_BLK へ
        r12(s, (START_BLK + 64) * 16, 16)
        r12(s, (START_BLK + 64) * 16, 16)
        run("back", [(START_BLK * 16, 16)])
    finally:
        s.close()


if __name__ == "__main__":
    main()
