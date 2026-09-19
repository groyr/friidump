#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Windows SPTI プローブ（GC ディスク / GCC-4240N 用）
- READ12 の streaming ビットで読み出しが成功するか
- 0xe7 メモリダンプのキャッシュ番地（5ブロック分）のマッピング
- E7 サイズ vs 所要時間、実サイクル（READ+E7）のスループット
DIC などがドライブを使用していない状態で実行すること。
"""

import ctypes
import struct
import time
from ctypes import wintypes

GENERIC_READ = 0x80000000
GENERIC_WRITE = 0x40000000
FILE_SHARE_READ = 0x00000001
FILE_SHARE_WRITE = 0x00000002
OPEN_EXISTING = 3
FILE_ATTRIBUTE_NORMAL = 0x80
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value


class SCSI_PASS_THROUGH_DIRECT(ctypes.Structure):
    _fields_ = [
        ("Length", ctypes.c_ushort),
        ("ScsiStatus", ctypes.c_ubyte),
        ("PathId", ctypes.c_ubyte),
        ("TargetId", ctypes.c_ubyte),
        ("Lun", ctypes.c_ubyte),
        ("CdbLength", ctypes.c_ubyte),
        ("SenseInfoLength", ctypes.c_ubyte),
        ("DataIn", ctypes.c_ubyte),
        ("DataTransferLength", ctypes.c_ulong),
        ("TimeOutValue", ctypes.c_ulong),
        ("DataBuffer", ctypes.c_void_p),
        ("SenseInfoOffset", ctypes.c_ulong),
        ("Cdb", ctypes.c_ubyte * 16),
    ]


IOCTL_SCSI_PASS_THROUGH_DIRECT = 0x0004D014
SCSI_IOCTL_DATA_OUT = 0
SCSI_IOCTL_DATA_IN = 1


class Spti:
    def __init__(self, letter="D"):
        self.k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self.k32.CreateFileW.restype = wintypes.HANDLE
        self.k32.CreateFileW.argtypes = [
            wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
            wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE,
        ]
        self.k32.DeviceIoControl.restype = wintypes.BOOL
        self.k32.DeviceIoControl.argtypes = [
            wintypes.HANDLE, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD,
            ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p,
        ]
        self.k32.CloseHandle.argtypes = [wintypes.HANDLE]
        self.h = self.k32.CreateFileW(
            "\\\\.\\%s:" % letter, GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL, None,
        )
        if self.h == INVALID_HANDLE_VALUE:
            raise OSError("CreateFile failed: %d" % ctypes.get_last_error())

    def close(self):
        self.k32.CloseHandle(self.h)

    def send(self, cdb, xfer_len, datain=True, timeout=10):
        sense_len = 32
        buf = ctypes.create_string_buffer(max(xfer_len, 1))
        info = SCSI_PASS_THROUGH_DIRECT()
        ctypes.memset(ctypes.byref(info), 0, ctypes.sizeof(info))
        info.Length = ctypes.sizeof(SCSI_PASS_THROUGH_DIRECT)
        info.CdbLength = len(cdb)
        info.SenseInfoLength = sense_len
        info.DataIn = SCSI_IOCTL_DATA_IN if datain else SCSI_IOCTL_DATA_OUT
        info.DataTransferLength = xfer_len
        info.TimeOutValue = timeout
        info.DataBuffer = ctypes.cast(buf, ctypes.c_void_p)
        info.SenseInfoOffset = ctypes.sizeof(SCSI_PASS_THROUGH_DIRECT)
        for i, b in enumerate(cdb):
            info.Cdb[i] = b
        out = ctypes.create_string_buffer(ctypes.sizeof(info) + sense_len)
        returned = wintypes.DWORD(0)
        t0 = time.perf_counter()
        ok = self.k32.DeviceIoControl(
            self.h, IOCTL_SCSI_PASS_THROUGH_DIRECT,
            ctypes.byref(info), ctypes.sizeof(info),
            out, ctypes.sizeof(out), ctypes.byref(returned), None,
        )
        dt = time.perf_counter() - t0
        oi = ctypes.cast(out, ctypes.POINTER(SCSI_PASS_THROUGH_DIRECT)).contents
        sense = out[ctypes.sizeof(SCSI_PASS_THROUGH_DIRECT):ctypes.sizeof(SCSI_PASS_THROUGH_DIRECT) + sense_len]
        return oi.ScsiStatus, sense, buf.raw[:xfer_len], dt, bool(ok)


def read12_cdb(lba, sectors, byte1=0x00, streaming=True):
    c = bytearray(12)
    c[0] = 0xA8
    c[1] = byte1
    c[2:6] = struct.pack(">I", lba)
    c[6:10] = struct.pack(">I", sectors)
    c[10] = 0x80 if streaming else 0x00  # streaming bit
    return bytes(c)


def e7_cdb(addr, length):
    c = bytearray(12)
    c[0] = 0xE7
    c[1] = 0x48
    c[2] = 0x49
    c[3] = 0x54
    c[4] = 0x01
    c[6] = (addr >> 24) & 0xFF
    c[7] = (addr >> 16) & 0xFF
    c[8] = (addr >> 8) & 0xFF
    c[9] = addr & 0xFF
    c[10] = (length >> 8) & 0xFF
    c[11] = length & 0xFF
    return bytes(c)


def sector_num(data):
    if len(data) < 4:
        return None
    return (data[1] << 16) | (data[2] << 8) | data[3]


def main():
    s = Spti("D")
    print("device opened")
    LBA = 0x1000
    base0 = 0x31000  # 0x30000 + LBA

    # A. READ12 streaming の可否
    print("\n[A] READ12 streaming check (LBA=0x%X, 16 sectors):" % LBA)
    for stream in (False, True):
        st, se, data, dt, ok = s.send(read12_cdb(LBA, 16, 0x00, streaming=stream), 32768, True, 20)
        print("  streaming=%s status=0x%02X sense=%s ok=%s %.4fs head=%s"
              % (stream, st, se[:4].hex(), ok, dt, data[:16].hex()))

    # B. 0xe7 キャッシュ番地マッピング（streaming READ 後の 5 ブロック）
    print("\n[B] E7 cache address map after streaming READ:")
    s.send(read12_cdb(LBA, 16, 0x00, streaming=True), 32768, True, 20)
    found = {}
    base = 0xA00000
    while base < 0xB40000:
        st, se, data, dt, ok = s.send(e7_cdb(base, 2064), 2064, True, 10)
        sn = sector_num(data)
        if sn is not None and base0 <= sn < base0 + 0x50:
            found[base] = sn
        base += 0x1000
    for b in sorted(found):
        print("  addr=0x%08X -> sn=0x%X (block %d)" % (b, found[b], (found[b] - base0) // 16))

    # ブロック j ごとの代表アドレス
    block_addr = {}
    for b, sn in found.items():
        j = (sn - base0) // 16
        if j not in block_addr:
            block_addr[j] = b
    print("  block->addr: %s" % {k: "0x%08X" % v for k, v in sorted(block_addr.items())})

    # C. E7 サイズ vs 所要時間（block0 のアドレスで）
    a0 = block_addr.get(0, 0xA13000)
    print("\n[C] E7 size vs time (addr=0x%08X):" % a0)
    for size in (1024, 4096, 16384, 33024, 65535):
        ts = []
        for _ in range(5):
            st, se, data, dt, ok = s.send(e7_cdb(a0, size), size, True, 10)
            ts.append(dt)
        ts.sort()
        print("  size=%6d status=0x%02X median=%.4fs" % (size, st, ts[len(ts) // 2]))

    # D. 実サイクル: streaming READ + E7(33024) を反復して実効スループット
    print("\n[D] real cycle (READ streaming + E7 33024) x20:")
    t0 = time.perf_counter()
    okc = 0
    for i in range(20):
        lba = LBA + i * 16
        st, se, data, dt, ok = s.send(read12_cdb(lba, 16, 0x00, streaming=True), 32768, True, 20)
        st2, se2, data2, dt2, ok2 = s.send(e7_cdb(a0, 33024), 33024, True, 10)
        if st == 0 and st2 == 0:
            okc += 1
    dt = time.perf_counter() - t0
    print("  %d ok, %.3fs, %.1f KB/s (raw 33024B/iter)" % (okc, dt, 20 * 33024 / 1024 / dt))

    s.close()


if __name__ == "__main__":
    main()
