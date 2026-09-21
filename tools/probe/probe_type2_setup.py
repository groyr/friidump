#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
GCC-4241N / GCC-4242N（Hitachi Type2, 0xe7）セットアップ・配置実測プローブ。

- INQUIRY で型番・FW を確認（メディア不要）
- READ(12) streaming が通るか
- 0x80000000 付近の E7 キャッシュ配置をスキャン（回転ベース・4セクタE7 の実測）

DIC など同じドライブを使うツールは停止した状態で実行すること。
既定デバイスは D:。引数でドライブレターを指定可能。
"""

import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from spti_probe import Spti, read12_cdb, e7_cdb, sector_num  # noqa: E402


def inquiry_cdb(alloc=36):
    c = bytearray(12)
    c[0] = 0x12          # INQUIRY
    c[1] = 0x00          # EVPD=0
    c[2] = 0x00
    c[4] = alloc         # allocation length
    return bytes(c)


def show_inquiry(s):
    print("\n[A] INQUIRY:")
    st, se, data, dt, ok = s.send(inquiry_cdb(36), 36, True, 10)
    if st != 0 or len(data) < 36:
        print("  status=0x%02X sense=%s ok=%s -> INQUIRY 失敗" % (st, se[:4].hex(), ok))
        return None
    vendor = data[8:16].decode("ascii", "replace").strip()
    product = data[16:32].decode("ascii", "replace").strip()
    rev = data[32:36].decode("ascii", "replace").strip()
    print("  vendor  = %r" % vendor)
    print("  product = %r" % product)
    print("  rev     = %r" % rev)
    print("  (%.4fs)" % dt)
    return (vendor, product, rev)


def check_read(s, lba=0x1000):
    print("\n[B] READ(12) streaming check (LBA=0x%X, 16 sectors):" % lba)
    st, se, data, dt, ok = s.send(read12_cdb(lba, 16, 0x00, streaming=True), 32768, True, 20)
    print("  status=0x%02X sense=%s ok=%s %.4fs head=%s"
          % (st, se[:4].hex(), ok, dt, data[:16].hex()))
    return st == 0


def scan_e7(s, base_lo, base_hi, step, tag):
    """base_lo..base_hi を step ずつ E7 で読み、セクタ番号が見つかった番地を返す。"""
    print("\n[%s] E7 scan 0x%08X..0x%08X step=0x%X:" % (tag, base_lo, base_hi, step))
    found = {}
    base = base_lo
    while base < base_hi:
        st, se, data, dt, ok = s.send(e7_cdb(base, 2064), 2064, True, 10)
        if st == 0 and len(data) >= 4:
            sn = sector_num(data)
            if sn is not None:
                found[base] = (sn, data[:4].hex())
        base += step
    if not found:
        print("  (セクタ番号らしきデータ無し)")
    else:
        for b in sorted(found):
            print("  addr=0x%08X sn=0x%X head=%s" % (b, found[b][0], found[b][1]))
    return found


def main():
    letter = sys.argv[1] if len(sys.argv) > 1 else "D"
    s = Spti(letter)
    print("device opened: %s:" % letter)
    try:
        info = show_inquiry(s)
        if info is None:
            return
        ok = check_read(s)
        if ok:
            # デシンク対策で READ をもう一度発行してから配置を見る
            s.send(read12_cdb(0x1000, 16, 0x00, streaming=True), 32768, True, 20)
            # 0x80000000 付近を細かくスキャン（DIC Type2 は 0x2040 回転）
            scan_e7(s, 0x80000000, 0x80008000, 0x800, "C1")
            # 少し広く粗くスキャン
            scan_e7(s, 0x80000000, 0x80040000, 0x2000, "C2")
        else:
            print("\n(READ 失敗: メディア未挿入か、ドライブが応答していません)")
    finally:
        s.close()


if __name__ == "__main__":
    main()
