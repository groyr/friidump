#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
キャッシュ遅延の意味を特定するプローブ（GCC-4240N / Initio）

確認したいこと:
  1. streaming READ のホストデータ rd は「要求ブロック」か「1つ前のブロック」か
  2. READ 直後の E7 キャッシュはどのブロックか（遅延段数）
  3. パイプライン構成（前回 rd + 今回 READ後の E7 前方6B）で 1 READ/ブロックにできるか

参照: DIC で取得済み gc2.iso（Mario Tennis GC）と比較する。
"""

import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"
START = 0x1000


def read_block(s, lba):
    st, se, d, dt, ok = s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
    return d, dt


def e7_sn(s, k=0):
    st, se, d, dt, ok = s.send(P.e7_cdb(BASE + k * 2064, 12), 12, True, 10)
    return P.sector_num(d)


def main():
    iso = open(ISO, "rb").read()

    def iso_block(blk):
        return iso[blk * 16 * 2048:(blk + 1) * 16 * 2048]

    s = P.Spti("D")
    try:
        print("=== 遅延の特性 (block, cached_sn, expected_sn, rd一致) ===")
        for i in range(6):
            blk = START // 16 + i
            lba = blk * 16
            st, se, rd, dt, ok = s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
            cached = e7_sn(s)
            exp = lba + 0x30000
            m_i = rd == iso_block(blk)
            m_prev = rd == iso_block(blk - 1)
            print("  blk=%d lba=0x%X cached_sn=0x%X exp=0x%X  rd==blk:%s rd==blk-1:%s (%.3fs)"
                  % (blk, lba, cached, exp, m_i, m_prev, dt))

        print("\n=== パイプライン検証: READ(i+1) -> E7(block i 前方6B) + 前回rd ===")
        # 初期化: block START を READ して rd を得る
        blk0 = START // 16
        prev_rd, _ = read_block(s, blk0 * 16)
        match = 0
        total = 0
        for i in range(1, 24):
            blk = blk0 + i
            lba = blk * 16
            rd, dt = read_block(s, lba)          # これは block i のホストデータ（のはず）
            # いま READ(i) 直後 → キャッシュは block i-1 のはず
            prefixes = []
            for k in range(16):
                st, se, d, dt2, ok = s.send(P.e7_cdb(BASE + k * 2064 + 6, 6), 6, True, 10)
                prefixes.append(d[:6])
            # 再構成: 前回 rd（=block i-1）の 2048B と、キャッシュ前方6Bを合成
            rebuilt = b"".join(prefixes[k] + prev_rd[k * 2048:(k + 1) * 2048 - 6] for k in range(16))
            # 正解は iso block (i-1) の先頭6B + rd の [6:2048]
            ref = iso_block(blk - 1)
            ref_body = b"".join(ref[k * 2048:(k + 1) * 2048] for k in range(16))
            okall = rebuilt == ref_body
            total += 1
            if okall:
                match += 1
            if i < 6:
                print("  i=%d (block %d): 一致=%s" % (i, blk - 1, okall))
            prev_rd = rd
        print("  パイプライン一致: %d/%d" % (match, total))
    finally:
        s.close()


if __name__ == "__main__":
    main()
