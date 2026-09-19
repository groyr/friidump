#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
H2: 通常のCD/DVDでREAD時間を測り、GCの80msが「GC固有（媒体起因）」か
「ブリッジ/ドライブ固定」かを切り分ける。

通常のデータCD/DVDを挿入してから実行すること。
"""
import statistics
import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P


def read_capacity(s):
    c = bytearray(12)
    c[0] = 0x25
    st, se, d, dt, ok = s.send(bytes(c), 8, True, 10)
    if st == 0 and len(d) >= 8:
        return (d[0] << 24) | (d[1] << 16) | (d[2] << 8) | d[3]
    return None


def r12(s, lba, n=16):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def settle(s, lba, n=16):
    r12(s, (lba + 0x10000))
    r12(s, lba)
    t0 = time.perf_counter()
    r12(s, lba, n)
    return time.perf_counter() - t0


def main():
    s = P.Spti("D")
    try:
        last = read_capacity(s)
        print("READ CAPACITY: last LBA = %s" % (hex(last) if last is not None else "不明"))
        if not last:
            print("メディアがありません。通常のCD/DVDを挿入してください。")
            return
        pts = [0x1000, last // 4, last // 2, (last * 3) // 4, max(0x1000, last - 0x2000)]
        print("settle READ (READ×2の2回目):")
        for lba in pts:
            ts = sorted(settle(s, lba) for _ in range(3))
            print("  lba=0x%06X median=%.4fs  (%.0f KB/s)"
                  % (lba, ts[1], 16 * 2048 / 1024 / ts[1]))
        print("サイズ依存 (内周):")
        for n in (16, 32, 64, 128):
            ts = sorted(settle(s, 0x1000, n) for _ in range(3))
            print("  READ(%3d) median=%.4fs" % (n, ts[1]))
    finally:
        s.close()


if __name__ == "__main__":
    main()
