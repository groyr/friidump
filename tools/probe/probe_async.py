#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
S4: overlapped SPTI で READ を非同期発行し、その間に前ブロックの E7 を処理できるか。

成功すれば per-block = max(READ 80ms, E7 11ms) + 次E7 ≒ 91ms（現行105ms）。
USB BOT では直列化され重畳しない可能性が高いが、実測で確認する。
"""
import ctypes
import struct
import sys
import time
from ctypes import wintypes

sys.path.insert(0, r"C:\Users\Umaaa\workspace\GC Ripper\scripts\probe")
import spti_probe as P

GENERIC_READ = 0x80000000
GENERIC_WRITE = 0x40000000
FILE_SHARE_READ = 0x01
FILE_SHARE_WRITE = 0x02
OPEN_EXISTING = 3
FILE_ATTRIBUTE_NORMAL = 0x80
FILE_FLAG_OVERLAPPED = 0x40000000
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value
ERROR_IO_PENDING = 997

IOCTL = 0x0004D014
SCSI_IOCTL_DATA_OUT = 0
SCSI_IOCTL_DATA_IN = 1

BASE = 0xA13000
A = 0x1000


class OVERLAPPED(ctypes.Structure):
    _fields_ = [
        ("Internal", ctypes.c_void_p),
        ("InternalHigh", ctypes.c_void_p),
        ("Offset", wintypes.DWORD),
        ("OffsetHigh", wintypes.DWORD),
        ("hEvent", wintypes.HANDLE),
    ]


class SPTD(ctypes.Structure):
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


class Async:
    def __init__(self, letter="D"):
        self.k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self.k32.CreateFileW.restype = wintypes.HANDLE
        self.k32.CreateEventW.restype = wintypes.HANDLE
        self.k32.CreateEventW.argtypes = [ctypes.c_void_p, wintypes.BOOL, wintypes.BOOL, wintypes.LPCWSTR]
        self.h = self.k32.CreateFileW(
            "\\\\.\\%s:" % letter, GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED, None,
        )
        if self.h == INVALID_HANDLE_VALUE:
            raise OSError("CreateFile failed: %d" % ctypes.get_last_error())

    def start(self, cdb, xfer_len, datain=True, timeout=10):
        sense_len = 32
        buf = ctypes.create_string_buffer(max(xfer_len, 1))
        sptd = SPTD()
        sptd.Length = ctypes.sizeof(SPTD)
        sptd.CdbLength = len(cdb)
        sptd.SenseInfoLength = sense_len
        sptd.DataIn = SCSI_IOCTL_DATA_IN if datain else SCSI_IOCTL_DATA_OUT
        sptd.DataTransferLength = xfer_len
        sptd.TimeOutValue = timeout
        sptd.DataBuffer = ctypes.cast(buf, ctypes.c_void_p)
        sptd.SenseInfoOffset = ctypes.sizeof(SPTD)
        for i, b in enumerate(cdb):
            sptd.Cdb[i] = b
        out = ctypes.create_string_buffer(ctypes.sizeof(SPTD) + sense_len)
        ov = OVERLAPPED()
        ov.hEvent = self.k32.CreateEventW(None, True, False, None)
        ret = wintypes.DWORD(0)
        t0 = time.perf_counter()
        ok = self.k32.DeviceIoControl(self.h, IOCTL, ctypes.byref(sptd), ctypes.sizeof(sptd),
                                      out, ctypes.sizeof(out), ctypes.byref(ret), ctypes.byref(ov))
        err = ctypes.get_last_error()
        if not ok and err != ERROR_IO_PENDING:
            raise OSError("DeviceIoControl failed: %d" % err)
        return {"ov": ov, "out": out, "buf": buf, "t0": t0, "pending": (not ok)}

    def wait(self, op):
        self.k32.WaitForSingleObject(op["ov"].hEvent, 30000)
        ret = wintypes.DWORD(0)
        self.k32.GetOverlappedResult(self.h, ctypes.byref(op["ov"]), ctypes.byref(ret), False)
        t1 = time.perf_counter()
        oi = ctypes.cast(op["out"], ctypes.POINTER(SPTD)).contents
        self.k32.CloseHandle(op["ov"].hEvent)
        return t1, oi.ScsiStatus, op["buf"].raw

    def close(self):
        self.k32.CloseHandle(self.h)


def main():
    s = Async("D")
    try:
        # プライム: block A を確定
        op = s.start(P.read12_cdb(A, 16, 0, streaming=True), 16 * 2048)
        _, _, _ = s.wait(op)
        t_p = time.perf_counter()
        op = s.start(P.read12_cdb(A, 16, 0, streaming=True), 16 * 2048)
        t_pend, _, _ = s.wait(op)
        print("プライム settle READ: %.4fs" % (t_pend - t_p))

        # 非同期 READ(A+0x20000)（遠方＝物理READが発生）を発行し、その間に E7(A) を別スレッドで処理
        import threading
        op_read = s.start(P.read12_cdb(A + 0x20000, 16, 0, streaming=True), 16 * 2048)
        t_read_start = time.perf_counter()

        e7_done = {}
        def do_e7():
            e7_done["t0"] = time.perf_counter()
            st, se, d, dt, ok = P.Spti("D").send(P.e7_cdb(BASE, 12), 12, True, 10)
            e7_done["t1"] = time.perf_counter()
            e7_done["sn"] = P.sector_num(d)
        th = threading.Thread(target=do_e7)
        th.start()
        t_end, status, _ = s.wait(op_read)
        th.join()

        print("非同期READ完了: status=0x%02X  %.4fs (start→done)" % (status, t_end - t_read_start))
        print("E7スレッド: start=+%.4fs end=+%.4fs sn=0x%X"
              % (e7_done["t0"] - t_read_start, e7_done["t1"] - t_read_start, e7_done["sn"]))
        overlap = e7_done["t1"] < t_end
        print("重畳: %s" % ("あり（E7がREAD完了前に終了）" if overlap else "なし（直列化）"))
    finally:
        s.close()


if __name__ == "__main__":
    main()
