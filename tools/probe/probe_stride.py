#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
26セクタ刻みストライド方式の検証。

仮説:
  - streaming READ 直後、0xA13000 から 26 セクタ分のキャッシュが有効
  - 前回窓と重ならないよう 26 セクタずつ進めれば、1 READ でキャッシュ確定
  - よって READ settle を 26 セクタに償却でき、速くなる

検証:
  A. READ(26) 後に何セクタ有効か
  B. 26 刻みサイクルのスループットと sector 一致率
  C. 16 刻み（現行相当、2 READ）との比較
"""

import sys
import time

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

BASE = 0xA13000
START = 0x1000
CHUNK = 26


def read_sectors(s, lba, n):
    return s.send(P.read12_cdb(lba, n, 0, streaming=True), n * 2048, True, 30)


def e7_prefix(s, k):
    return s.send(P.e7_cdb(BASE + k * 2064, 12), 12, True, 10)


def scan_after_read(s, lba, read_n, scan_n=48):
    s.send(P.read12_cdb(START + 0x8000, 16, 0, streaming=True), 16 * 2048, True, 30)
    st, se, d, dt, ok = read_sectors(s, lba, read_n)
    valid = []
    for k in range(scan_n):
        st2, se2, d2, dt2, ok2 = e7_prefix(s, k)
        sn = P.sector_num(d2)
        if sn is not None and sn == lba + k + 0x30000 and (d2[0] & 1) == 0:
            valid.append(k)
    return valid, dt


def cycle_stride(s, lba, sectors, chunk):
    """chunk セクタずつ READ し、各セクタ先頭12Bを E7 で読む。sector一致を検証。"""
    done = 0
    good = 0
    t0 = time.perf_counter()
    while done < sectors:
        n = min(chunk, sectors - done)
        cur = lba + done
        st, se, rd, dt, ok = read_sectors(s, cur, n)
        for k in range(n):
            st2, se2, d2, dt2, ok2 = e7_prefix(s, k)
            sn = P.sector_num(d2)
            if sn == cur + k + 0x30000:
                good += 1
        done += n
    return time.perf_counter() - t0, good


def cycle_stride_2read(s, lba, sectors, chunk):
    """比較用: 各チャンクで READ×2"""
    done = 0
    good = 0
    t0 = time.perf_counter()
    while done < sectors:
        n = min(chunk, sectors - done)
        cur = lba + done
        read_sectors(s, cur, n)
        st, se, rd, dt, ok = read_sectors(s, cur, n)
        for k in range(n):
            st2, se2, d2, dt2, ok2 = e7_prefix(s, k)
            sn = P.sector_num(d2)
            if sn == cur + k + 0x30000:
                good += 1
        done += n
    return time.perf_counter() - t0, good


def main():
    s = P.Spti("D")
    try:
        print("=== A. READ(26) 後の有効セクタ ===")
        for rn in (16, 24, 26):
            valid, dt = scan_after_read(s, START, rn)
            print("  READ(%2d) %.3fs -> valid offsets=%s" % (rn, dt, valid))

        print("\n=== B/C. サイクル比較 (sectors=2600, LBA 0x%X) ===" % START)
        for name, fn in (
            ("stride26  READx1", lambda: cycle_stride(s, START, 2600, 26)),
            ("stride26  READx2", lambda: cycle_stride_2read(s, START, 2600, 26)),
            ("stride16  READx2", lambda: cycle_stride_2read(s, START, 2600, 16)),
        ):
            dt, good = fn()
            mb = 2600 * 2064 / 1024 / 1024
            print("  %-18s %.3fs  %.2f MB/s  %.3f ms/sector  sector一致 %d/2600"
                  % (name, dt, mb / dt, dt / 2600 * 1000, good))
    finally:
        s.close()


if __name__ == "__main__":
    main()
