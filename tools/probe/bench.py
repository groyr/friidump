#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
読み出し方式ベンチマーク（GCC-4240N / Initio 13FD:1040）

E2: オペコード別レイテンシ（READ / PREFETCH / E7）
E4: method10（READ + E7 33024 一括）vs method11（READ + E7 12B×16）の実測比較
E5: E7 サイズ別レイテンシ

DiscImageCreator 等、同じドライブを使うツールは終了しておくこと。
"""

import argparse
import statistics
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
BLOCK_SECTORS = 16


def median_min_max(ts):
    return (statistics.median(ts), min(ts), max(ts))


def measure_e7(s, addr, size, n=8):
    ts = []
    for _ in range(n):
        st, se, d, dt, ok = s.send(P.e7_cdb(addr, size), size, True, 10)
        ts.append(dt)
    med, mn, mx = median_min_max(ts)
    return med, mn, mx, ts


def measure_read(s, lba, sectors, n=8, streaming=True):
    xfer = sectors * 2048
    ts = []
    for _ in range(n):
        st, se, d, dt, ok = s.send(P.read12_cdb(lba, sectors, 0, streaming=streaming), xfer, True, 30)
        ts.append(dt)
    med, mn, mx = median_min_max(ts)
    return med, mn, mx, ts


def e7_prefix(s, k):
    return s.send(P.e7_cdb(BASE + k * 2064, 12), 12, True, 10)


def cycle_method10(s, blocks, start_lba, verify=True):
    """streaming READ + E7 33024 一括"""
    ok = 0
    t0 = time.perf_counter()
    for i in range(blocks):
        lba = start_lba + i * BLOCK_SECTORS
        s.send(P.read12_cdb(lba, 16, 0, streaming=True), BLOCK_SECTORS * 2048, True, 30)
        st, se, d, dt, okc = s.send(P.e7_cdb(BASE, 33024), 33024, True, 10)
        if verify and okc and len(d) >= 4:
            sn = P.sector_num(d)
            if sn == lba + 0x30000:
                ok += 1
        else:
            ok += 1
    dt = time.perf_counter() - t0
    return dt, ok


def cycle_method11(s, blocks, start_lba, prefetch=False, double=False, verify=True):
    """1 ブロック: READ(rd) [+PREFETCH/+READ] + E7 12B×16"""
    ok = 0
    t0 = time.perf_counter()
    for i in range(blocks):
        lba = start_lba + i * BLOCK_SECTORS
        if double:
            s.send(P.read12_cdb(lba, 16, 0, streaming=True), BLOCK_SECTORS * 2048, True, 30)
        st, se, rd, dt, okc = s.send(P.read12_cdb(lba, 16, 0, streaming=True), BLOCK_SECTORS * 2048, True, 30)
        if prefetch:
            # PREFETCH(10): 0x34, LBA, sectors
            c = bytearray(12)
            c[0] = 0x34
            c[2:6] = lba.to_bytes(4, "big")
            c[7:10] = (16).to_bytes(3, "big")
            s.send(bytes(c), 0, True, 10)
        good = True
        for k in range(16):
            st2, se2, d2, dt2, ok2 = e7_prefix(s, k)
            if verify:
                sn = P.sector_num(d2)
                if sn != lba + k + 0x30000:
                    good = False
        ok += 1 if good else 0
    dt = time.perf_counter() - t0
    return dt, ok


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--blocks", type=int, default=64)
    ap.add_argument("--start", type=lambda x: int(x, 0), default=0x1000)
    ap.add_argument("--quick", action="store_true")
    a = ap.parse_args()

    s = P.Spti("D")
    try:
        print("=== E2: READ レイテンシ (LBA 0x%X) ===" % a.start)
        for sec in (16, 26, 32):
            med, mn, mx, _ = measure_read(s, a.start, sec, n=(3 if a.quick else 6))
            print("  READ streaming %2d sectors: median=%.4fs min=%.4fs max=%.4fs" % (sec, med, mn, mx))
        st, se, d, dt, okc = s.send(P.read12_cdb(a.start, 16, 0, streaming=True), 16 * 2048, True, 30)
        st2, se2, d2, dt2, okc2 = s.send(P.read12_cdb(a.start, 16, 0, streaming=True), 16 * 2048, True, 30)
        print("  連続 READ1=%.4fs READ2=%.4fs (同一LBA)" % (dt, dt2))

        print("\n=== E5: E7 サイズ別レイテンシ (base 0x%X) ===" % BASE)
        for size in (6, 12, 2064, 33024, 65535):
            med, mn, mx, _ = measure_e7(s, BASE, size, n=(4 if a.quick else 8))
            kbs = size / 1024 / med if med > 0 else 0
            print("  E7 size=%6d: median=%.4fs min=%.4fs max=%.4fs  (%.0f KB/s)" % (size, med, mn, mx, kbs))

        print("\n=== E4: 方式別サイクル (%d ブロック, LBA 0x%X〜) ===" % (a.blocks, a.start))
        variants = [
            ("method10 (READ + E7 33024)", lambda b, l: cycle_method10(s, b, l, verify=not a.quick)),
            ("method11 (READ + E7 12x16)", lambda b, l: cycle_method11(s, b, l, verify=not a.quick)),
            ("method11 + PREFETCH", lambda b, l: cycle_method11(s, b, l, prefetch=True, verify=not a.quick)),
            ("method11 + READx2", lambda b, l: cycle_method11(s, b, l, double=True, verify=not a.quick)),
        ]
        for name, fn in variants:
            dt, ok = fn(a.blocks, a.start)
            mb = a.blocks * 33024 / 1024 / 1024
            per_block = dt / a.blocks
            print("  %-28s %.3fs  %.2f MB/s  %.1f ms/block  sector一致 %d/%d"
                  % (name, dt, mb / dt, per_block * 1000, ok, a.blocks))
    finally:
        s.close()


if __name__ == "__main__":
    main()
