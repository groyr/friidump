#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# スクランブル補正 (raw^iso) が 256 セクタ周期で一致するかを確認
RAW = r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw"
ISO = r"C:\Users\Umaaa\workspace\DIC\out\gc2.iso"

raw = open(RAW, "rb").read()
iso = open(ISO, "rb").read()


def corr(lba):
    r = raw[lba * 2064:lba * 2064 + 2064]
    p = iso[lba * 2048:lba * 2048 + 2048]
    return bytes(a ^ b for a, b in zip(r[12:2060], p))


print("k : corr[k] == corr[k+256] == corr[k+4096] ?")
for k in range(0, 16):
    a = corr(k)
    b = corr(k + 256)
    c = corr(k + 4096)
    print("%2d: 256:%s 4096:%s  (head=%s)" % (k, a == b, a == c, a[:8].hex()))

# 追加: 4096 と 256 も比較
for k in (0, 1, 5, 8):
    print("k=%d 256vs4096:%s" % (k, corr(k + 256) == corr(k + 4096)))
