#!/usr/bin/env python3
"""ISO の CRC-32 / MD5 / SHA-1 を 1 パスで計算する（redump 照合用）。
使い方: hash-iso.py <file>
"""
import hashlib
import sys
import zlib

path = sys.argv[1]
md5 = hashlib.md5()
sha1 = hashlib.sha1()
crc = 0
size = 0
with open(path, "rb") as f:
    while True:
        b = f.read(1 << 20)
        if not b:
            break
        size += len(b)
        md5.update(b)
        sha1.update(b)
        crc = zlib.crc32(b, crc)

print(f"size={size}")
print(f"crc32={crc & 0xFFFFFFFF:08x}")
print(f"md5={md5.hexdigest()}")
print(f"sha1={sha1.hexdigest()}")
