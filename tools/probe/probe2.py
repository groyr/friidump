#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# streaming READ 後に、0xe7 で 5 ブロック分が連続して取れるかを確認する
import spti_probe as P

s = P.Spti("D")
LBA = 0x1000
base0 = 0x30000 + LBA

for rd in (16, 32, 80):
    s.send(P.read12_cdb(LBA, rd, 0x00, streaming=True), rd * 2048, True, 30)
    print("\n=== after streaming READ %d sectors ===" % rd)
    for a0 in (0xA13000, 0xA08000):
        sns = []
        for k in range(80):
            st, se, data, dt, ok = s.send(P.e7_cdb(a0 + k * 2064, 2064), 2064, True, 10)
            sns.append(P.sector_num(data))
        ok = sum(1 for i, sn in enumerate(sns) if sn == base0 + i)
        # 連続して一致している最大長
        run = 0
        for i, sn in enumerate(sns):
            if sn == base0 + i:
                run += 1
            else:
                break
        print("  base=0x%08X match=%d/80 run=%d first=%s" %
              (a0, ok, run, ["0x%X" % x for x in sns[:6]]))

s.close()
