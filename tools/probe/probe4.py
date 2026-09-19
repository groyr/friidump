#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# streaming READ の 2048B が「iso の XOR スクランブル」なのかを seed 全探索で判定
import spti_probe as P

RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"


def lfsr_bytes(seed, n):
    L = seed & 0x7FFF
    out = bytearray(n)
    for i in range(n):
        b = 0
        for _ in range(8):
            ret = (L >> 14) & 1
            nb = ret ^ ((L >> 10) & 1)
            L = ((L << 1) | nb) & 0x7FFF
            b = (b << 1) | ret
        out[i] = b
    return bytes(out)


def search(x, target, off=0, n=8):
    for seed in range(0x8000):
        c = lfsr_bytes(seed, n)
        if all((x[off + i] ^ c[i]) == target[i] for i in range(n)):
            return seed
    return None


def main():
    s = P.Spti("D")
    raw = open(RAW, "rb").read()
    iso = open(ISO, "rb").read()
    for lba in (0, 0x1000):
        st, se, data, dt, ok = s.send(P.read12_cdb(lba, 16, 0x00, streaming=True), 16 * 2048, True, 30)
        rd = data[:2048]
        isos = iso[lba * 2048:lba * 2048 + 2048]
        raw12 = raw[lba * 2064 + 12:lba * 2064 + 12 + 2048]
        x_iso = bytes(a ^ b for a, b in zip(rd, isos))
        x_raw = bytes(a ^ b for a, b in zip(rd, raw12))
        print("LBA=0x%X" % lba)
        print("  X=rd^iso :", x_iso[:16].hex())
        print("  X=rd^raw :", x_raw[:16].hex())
        print("  seed for X=rd^iso (off0):", search(x_iso, b"\x00" * 8))
        print("  seed for X=rd^raw (off0):", search(x_raw, b"\x00" * 8))
        # 先頭バイト一致だけで seed 候補を見る（X 自体が cipher の場合）
        cand = [sd for sd in range(0x8000) if lfsr_bytes(sd, 1)[0] == x_iso[0]]
        print("  X=rd^iso[0]=0x%02X -> seed候補数=%d" % (x_iso[0], len(cand)))
    s.close()


if __name__ == "__main__":
    main()
