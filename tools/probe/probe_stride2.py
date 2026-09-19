#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
method11 の校正(corr[phase])を使い、READ(26) のホストデータが
26セクタ分（キャッシュ窓全体）正しいかを検証する。

正しければ 26 セクタ刻みで 1物理READ/26セクタ になり、大幅高速化できる。

corr[m][i] = rd[i] XOR iso_sector[6+i]  （ディスク==gc2 を利用）
plain_sec  = raw[6:12](E7) + (rd[i] XOR corr[phase])
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
CAL_BLK = 16
START = 0x1000


def r12(s, lba, n):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def main():
    iso = open(ISO, "rb").read()

    def iso_sec(sec):
        return iso[sec * 2048:(sec + 1) * 2048]

    s = P.Spti("D")
    try:
        # --- 校正: blocks 16..31 ---
        corr = [[0] * 2042 for _ in range(16)]
        for m in range(16):
            blk = CAL_BLK + m
            lba = blk * 16
            r12(s, lba, 16)
            rd = r12(s, lba, 16)[2]
            p = iso_sec(lba)  # sector 0 of block
            for i in range(2042):
                corr[m][i] = rd[i] ^ p[6 + i]
        print("校正完了 (16 phases)")

        def recover(rd, sec, cache_base):
            m = (sec // 16) % 16
            off = sec - cache_base
            if off < 0 or off > 25:
                return None
            pre = s.send(P.e7_cdb(BASE + off * 2064 + 6, 6), 6, True, 10)[2][:6]
            C = corr[m]
            body = bytes(rd[i] ^ C[i] for i in range(2042))
            return pre + body

        def test(lba, n):
            cache_base = (lba // 16) * 16
            # リセット
            r12(s, START + 0x4000, 16)
            r12(s, lba, 16)
            t0 = time.perf_counter()
            rd = r12(s, lba, n)[2]
            dt = time.perf_counter() - t0
            match = 0
            bad = []
            for k in range(n):
                sec = lba + k
                got = recover(rd[k * 2048:(k + 1) * 2048], sec, cache_base)
                if got == iso_sec(sec):
                    match += 1
                else:
                    bad.append(k)
            print("  READ(%2d) lba=0x%X (cache_base=0x%X): 一致 %d/%d  不一致=%s  %.3fs"
                  % (n, lba, cache_base, match, n, bad[:6], dt))

        print("=== READ(26) のホストデータ検証（corr使用） ===")
        test(START, 26)            # ブロック整列
        test(START + 16, 26)       # 次ブロック整列
        test(START + 26, 26)       # 非整列
        test(START + 16, 16)       # 16セクタ
    finally:
        s.close()


if __name__ == "__main__":
    main()
