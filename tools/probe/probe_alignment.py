#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""READ 後のキャッシュ先頭がどのセクタに整列するかを調べる。"""
import sys
sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
START = 0x1000


def reset(s):
    s.send(P.read12_cdb(START + 0x8000, 16, 0, streaming=True), 16 * 2048, True, 30)


def probe(s, lba, read_n):
    reset(s)
    st, se, d, dt, ok = s.send(P.read12_cdb(lba, read_n, 0, streaming=True), read_n * 2048, True, 30)
    out = []
    for k in range(40):
        st2, se2, d2, dt2, ok2 = s.send(P.e7_cdb(BASE + k * 2064, 12), 12, True, 10)
        sn = P.sector_num(d2)
        valid = sn is not None and (d2[0] & 1) == 0
        out.append((k, sn - (START + 0x30000) if valid else None))
    print("READ(%2d) at offset %2d (lba 0x%X):" % (read_n, lba - START, lba))
    print("  " + ", ".join("%d:%s" % (k, v) for k, v in out[:34]))


def main():
    s = P.Spti("D")
    try:
        for off in (0, 8, 10, 16, 26, 32):
            probe(s, START + off, 16)
    finally:
        s.close()


if __name__ == "__main__":
    main()
