#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
0xA13000 からの連続セクタ読み出しでキャッシュ内容（何ブロック分か）を調べる。

READ のセクタ数を変えて、E7 で 0xA13000 + j*2064 の sector number を順に読み、
どの範囲が有効かを可視化する。
"""

import sys

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
START = 0x1000
MAXS = 96  # 探査するセクタ数


def scan(s, read_sectors):
    # リセットのため少し離れた場所を READ
    s.send(P.read12_cdb(START + 0x2000, 16, 0, streaming=True), 16 * 2048, True, 30)
    st, se, d, dt, ok = s.send(P.read12_cdb(START, read_sectors, 0, streaming=True),
                               read_sectors * 2048, True, 30)
    seq = []
    for j in range(MAXS):
        st2, se2, d2, dt2, ok2 = s.send(P.e7_cdb(BASE + j * 2064, 12), 12, True, 10)
        sn = P.sector_num(d2)
        parity = d2[0] & 1 if len(d2) else -1
        seq.append((j, sn, parity))
    print("READ %d sectors (%.3fs):" % (read_sectors, dt))
    run = []
    for j, sn, parity in seq:
        if sn is not None and START + 0x30000 <= sn < START + 0x30000 + MAXS + read_sectors and parity == 0:
            run.append((j, sn - (START + 0x30000)))
        else:
            if run:
                print("  sectors %s" % (run,))
                run = []
    if run:
        print("  sectors %s" % (run,))


def main():
    s = P.Spti("D")
    try:
        for n in (16, 32, 80):
            scan(s, n)
    finally:
        s.close()


if __name__ == "__main__":
    main()
