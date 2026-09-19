#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
GC: 単発連続READの検証（窓移動・settleなし）

狙い: 連続する LBA を「1回のREAD」だけで読み、ホストデータが corr で
正しく復号できるか（＝デシンクはE7キャッシュだけの問題か）を確かめる。

corr[m][i] = host[i] XOR iso[6+i]（ディスク==gc2 を利用）
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
BASE = 0xA13000
START_BLK = 256
NBLK = 48


def r12(s, lba, n):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def main():
    iso = open(ISO, "rb").read()

    def iso_sec(sec):
        return iso[sec * 2048:(sec + 1) * 2048]

    s = P.Spti("D")
    try:
        # 校正: 位相 m は block 16+m（READ×2で確定）
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

        def run(name, step, n):
            # ウォームアップ
            for _ in range(3):
                r12(s, (START_BLK - 4) * 16, n)
            lba = START_BLK * 16
            blk = START_BLK
            t0 = time.perf_counter()
            good_sec = 0
            tot_sec = 0
            good_blk = 0
            nblk = 0
            while nblk < NBLK:
                rd = r12(s, lba, n)[2]
                okb = True
                for k in range(n):
                    sec = lba + k
                    if body_ok(rd[k * 2048:(k + 1) * 2048], sec):
                        good_sec += 1
                    else:
                        okb = False
                    tot_sec += 1
                good_blk += 1 if okb else 0
                lba += step
                nblk += 1
            dt = time.perf_counter() - t0
            mb = nblk * step * 2048 / 1048576
            print("%-10s %d回 step=%d: %.2f MB/s  %.1f ms/iter  セクタ一致 %d/%d  ブロック全一致 %d/%d"
                  % (name, nblk, step, mb / dt, dt / nblk * 1000, good_sec, tot_sec, good_blk, nblk))

        # 連続: +16 / +26 / +32
        run("seq16", 16, 16)
        run("seq26", 26, 26)
        run("seq32", 32, 26)
    finally:
        s.close()


if __name__ == "__main__":
    main()
