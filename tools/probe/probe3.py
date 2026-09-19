#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# 通常READ(streaming) の 2048B が plain / スクランブル どちらかを DIC の raw/iso と比較して判定する
import spti_probe as P

RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"


def lfsr_stream(seed, n=2048):
    L = seed & 0x7FFF
    out = bytearray()
    for _ in range(n):
        b = 0
        for _ in range(8):
            ret = (L >> 14) & 1
            nbit = ret ^ ((L >> 10) & 1)
            L = ((L << 1) | nbit) & 0x7FFF
            b = ((b << 1) | ret) & 0xFF
        out.append(b)
    return bytes(out)


def main():
    s = P.Spti("D")
    with open(RAW, "rb") as f:
        raw = f.read()
    with open(ISO, "rb") as f:
        iso = f.read()

    for lba in (0, 1, 0x1000, 0x40000):
        st, se, data, dt, ok = s.send(P.read12_cdb(lba, 16, 0x00, streaming=True), 16 * 2048, True, 30)
        rd = data[:2048]
        rawf = raw[lba * 2064:lba * 2064 + 2064]
        isos = iso[lba * 2048:lba * 2048 + 2048]
        a = rd == isos
        b = rd == rawf[12:12 + 2048]
        c = rd == rawf[6:6 + 2048]
        print("LBA=0x%X status=0x%02X plain(iso)=%s raw[12:]=%s raw[6:]=%s" % (lba, st, a, b, c))
        print("   read head:", rd[:16].hex())
        print("   iso  head:", isos[:16].hex())
        print("   raw12head:", rawf[12:28].hex())
        # スクランブル仮定: rd(=raw[12:]) XOR cipher == iso[6:]
        # まず先頭 8 バイトで seed を高速探索し、見つかれば全体を検証
        if not a and b:
            found = None
            for seed in range(0x8000):
                c = lfsr_stream(seed, 8)
                if all((rd[i] ^ c[i]) == isos[6 + i] for i in range(8)):
                    found = seed
                    break
            if found is not None:
                cfull = lfsr_stream(found, 2048)
                full = bytes(rd[i] ^ cfull[i] for i in range(2042))
                print("   -> seed=0x%04X full_match=%s" % (found, full == isos[6:2048]))
            else:
                print("   -> seed not found (first 8 bytes, 0..0x7FFF)")
    s.close()


if __name__ == "__main__":
    main()
