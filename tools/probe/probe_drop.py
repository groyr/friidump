#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
メディア取りこぼし（トレイ開/媒体消失）の切り分け。
E7連発 / READ連発 / アイドル でどこで落ちるかを確認する。
実行中はトレイを目視監視すること。
"""
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000


def tur(s):
    st, se, d, dt, ok = s.send(bytes([0x00]), 0, True, 5)
    return st, se[12], se[13]


def main():
    s = P.Spti("D")
    try:
        st, asc, ascq = tur(s)
        print("start TUR st=0x%02X asc=%02X/%02X" % (st, asc, ascq), flush=True)

        print("--- Phase1: E7 33024 x30 ---", flush=True)
        for i in range(30):
            st2, se2, d2, dt2, ok2 = s.send(P.e7_cdb(BASE, 33024), 33024, True, 10)
            st, asc, ascq = tur(s)
            print("  E7 %2d: st=0x%02X drop=%s" % (i, st2, st != 0), flush=True)
            if st != 0:
                print("  -> drop during E7 (asc=%02X/%02X)" % (asc, ascq), flush=True)
                return
            time.sleep(0.2)

        print("--- Phase2: READ x2 x30 (lba inc) ---", flush=True)
        lba = 0x1000
        for i in range(30):
            s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
            st2, se2, d2, dt2, ok2 = s.send(P.read12_cdb(lba, 16, 0, streaming=True), 16 * 2048, True, 30)
            st, asc, ascq = tur(s)
            print("  READ %2d lba=0x%X: st=0x%02X drop=%s" % (i, lba, st2, st != 0), flush=True)
            if st != 0:
                print("  -> drop during READ (asc=%02X/%02X)" % (asc, ascq), flush=True)
                return
            lba += 16
            time.sleep(0.2)

        print("--- Phase3: idle 30s ---", flush=True)
        for i in range(15):
            time.sleep(2)
            st, asc, ascq = tur(s)
            if st != 0:
                print("  -> drop during idle after %ds (asc=%02X/%02X)" % (i * 2 + 2, asc, ascq), flush=True)
                return
        print("no drop (all phases passed)", flush=True)
    finally:
        s.close()


if __name__ == "__main__":
    main()
