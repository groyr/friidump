#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# streaming READ 後のキャッシュから、各セクタ先頭6バイト(raw[6:12])を E7 6バイトで取得できるか
import time
import spti_probe as P

raw = open(r"C:\Users\Umaaa\workspace\DIC\out\gc2.raw", "rb").read()
s = P.Spti("D")
LBA = 0x1000
BASE = 0xA13000

st, se, data, dt, ok = s.send(P.read12_cdb(LBA, 16, 0, streaming=True), 16 * 2048, True, 30)
print("read status=0x%02X %.3fs" % (st, dt))
match = 0
ts = []
for k in range(16):
    addr = BASE + k * 2064 + 6
    t0 = time.perf_counter()
    st2, se2, d2, dt2, ok2 = s.send(P.e7_cdb(addr, 6), 6, True, 10)
    ts.append(time.perf_counter() - t0)
    exp = raw[(LBA + k) * 2064 + 6:(LBA + k) * 2064 + 12]
    if d2[:6] == exp:
        match += 1
print("prefix match=%d/16  avg6B=%.4fs" % (match, sum(ts) / len(ts)))

# 比較: ブロック全体を1回のE7で
t0 = time.perf_counter()
st3, se3, d3, dt3, ok3 = s.send(P.e7_cdb(BASE, 33024), 33024, True, 10)
print("full-block E7 %.4fs" % (time.perf_counter() - t0))
# 16回の6B E7 合計 vs 全ブロック1回
print("sum16x6B=%.4fs" % sum(ts))
s.close()
