#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# READ CD (0xBE) の各種フラグで 2064 フレーム等が取れるか試す
import struct
import spti_probe as P

CDB_BE = None


def be_cdb(lba, blocks, flags, sect_type=0x00, sub=0x00):
    c = bytearray(12)
    c[0] = 0xBE
    c[1] = (sect_type & 0x07) << 2
    c[2:6] = struct.pack(">I", lba)
    c[6] = (blocks >> 16) & 0xFF
    c[7] = (blocks >> 8) & 0xFF
    c[8] = blocks & 0xFF
    c[9] = flags
    c[10] = sub
    return bytes(c)


def main():
    s = P.Spti("D")
    raw = open(r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw", "rb").read()
    lba = 0x1000
    rawf = raw[lba * 2064:lba * 2064 + 2064]
    combos = [
        (0x10, 2048),   # user data
        (0xF8, 2352),   # sync+header+user+edc+ecc+sub
        (0x28, 2352),   # sync+edc
        (0x08, 2352),   # header
        (0x18, 2352),   # header+user
        (0x14, 2352),   # user+edc? 
        (0xF0, 2352),
        (0x30, 2352),   # sync+user
    ]
    for flags, size in combos:
        cdb = be_cdb(lba, 1, flags)
        st, se, data, dt, ok = s.send(cdb, size, True, 20)
        head = data[:16].hex()
        print("flags=0x%02X size=%4d status=0x%02X sense=%s %.3fs head=%s" %
              (flags, size, st, se[:4].hex(), dt, head))
        # 2064 生フレームとの比較（先頭一致）
        if data[:8] == rawf[:8]:
            print("    == raw frame prefix")
    s.close()


if __name__ == "__main__":
    main()
