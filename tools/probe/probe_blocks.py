#!/usr/bin/env python3
"""指定ブロックの raw フレームを E7 から取得し、seed を線形探索で求めて EDC を検証する。
使い方: python3 probe_blocks.py 9856 9884 ...
"""
import ctypes, os, struct, sys

libc = ctypes.CDLL("libc.so.6", use_errno=True)
libc.ioctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_void_p]
libc.ioctl.restype = ctypes.c_int
SG_IO = 0x2285
FROMDEV = -3


class H(ctypes.Structure):
    _fields_ = [
        ("interface_id", ctypes.c_int), ("dxfer_direction", ctypes.c_int),
        ("cmd_len", ctypes.c_ubyte), ("mx_sb_len", ctypes.c_ubyte),
        ("iovec_count", ctypes.c_ushort), ("dxfer_len", ctypes.c_uint),
        ("dxferp", ctypes.c_void_p), ("cmdp", ctypes.c_void_p),
        ("sbp", ctypes.c_void_p), ("timeout", ctypes.c_uint),
        ("flags", ctypes.c_uint), ("pack_id", ctypes.c_int),
        ("usr_ptr", ctypes.c_void_p), ("status", ctypes.c_ubyte),
        ("masked_status", ctypes.c_ubyte), ("msg_status", ctypes.c_ubyte),
        ("sb_len_wr", ctypes.c_ubyte), ("host_status", ctypes.c_ushort),
        ("driver_status", ctypes.c_ushort), ("resid", ctypes.c_int),
        ("duration", ctypes.c_uint), ("info", ctypes.c_uint),
    ]


def sg(fd, cmd, dl):
    data = ctypes.create_string_buffer(max(dl, 1))
    sense = ctypes.create_string_buffer(32)
    cb = ctypes.create_string_buffer(cmd, len(cmd))
    h = H()
    h.interface_id = 0x53
    h.dxfer_direction = FROMDEV
    h.cmd_len = len(cmd)
    h.mx_sb_len = 32
    h.dxfer_len = dl
    h.dxferp = ctypes.addressof(data)
    h.cmdp = ctypes.addressof(cb)
    h.sbp = ctypes.addressof(sense)
    h.timeout = 10000
    r = libc.ioctl(fd, SG_IO, ctypes.byref(h))
    if r < 0:
        return "err", b""
    s = bytes(sense.raw)
    return ("ok" if h.status == 0 else "fail"), bytes(data.raw)


def stream(fd, lba, n=16):
    c = bytearray(12)
    c[0] = 0xA8
    c[2:6] = [(lba >> 24) & 0xFF, (lba >> 16) & 0xFF, (lba >> 8) & 0xFF, lba & 0xFF]
    c[9] = n
    c[10] = 0x80
    return sg(fd, bytes(c), 2048 * n)


def e7(fd, addr, n):
    c = bytearray(12)
    c[0] = 0xE7
    c[1], c[2], c[3], c[4] = 0x48, 0x49, 0x54, 0x01
    c[6:10] = [(addr >> 24) & 0xFF, (addr >> 16) & 0xFF, (addr >> 8) & 0xFF, addr & 0xFF]
    c[10] = (n >> 8) & 0xFF
    c[11] = n & 0xFF
    return sg(fd, bytes(c), n)


TAB = [0] * 256
for i in range(256):
    c = (i << 24) & 0xFFFFFFFF
    for _ in range(8):
        c = (((c << 1) ^ 0x80000011) & 0xFFFFFFFF) if (c & 0x80000000) else ((c << 1) & 0xFFFFFFFF)
    TAB[i] = c


def edc(data, edc=0):
    for b in data:
        edc = TAB[((edc >> 24) ^ b) & 0xFF] ^ ((edc << 8) & 0xFFFFFFFF)
    return edc & 0xFFFFFFFF


class L:
    __slots__ = ("s",)

    def __init__(self, seed):
        self.s = seed

    def byte(self):
        r = 0
        for _ in range(8):
            t = self.s >> 14
            n = t ^ ((self.s >> 10) & 1)
            self.s = ((self.s << 1) | n) & 0x7FFF
            r = (r << 1) | t
        return r


def cipher(seed):
    l = L(seed)
    return bytes(l.byte() for _ in range(2048))


E = [0] * 15
for k in range(15):
    pat = bytearray(2060)
    pat[12:] = cipher(1 << k)
    E[k] = edc(bytes(pat))


def find_seed(raw):
    base = edc(raw[0:2060])
    correct = struct.unpack(">I", raw[2060:2064])[0]
    for s in range(0x7FFF):
        acc = 0
        x = s
        k = 0
        while x:
            if x & 1:
                acc ^= E[k]
            x >>= 1
            k += 1
        if (base ^ acc) == correct:
            return s
    return None


fd = os.open("/dev/sg0", os.O_RDWR)
total = 4155840
for blk in [int(a) for a in sys.argv[1:]]:
    lba = blk * 16
    stream(fd, lba)
    st, _ = stream(fd, lba)
    st2, raw = e7(fd, 0xA13000, 33024)
    sn0 = struct.unpack(">I", b"\x00" + raw[1:4])[0]
    seed = find_seed(raw)
    okc = 0
    if seed is not None:
        cph = cipher(seed)
        for j in range(16):
            ro = j * 2064
            tmp = bytearray(raw[ro:ro + 2064])
            for i in range(2048):
                tmp[12 + i] ^= cph[i]
            if edc(bytes(tmp[0:2060])) == struct.unpack(">I", bytes(tmp[2060:2064]))[0]:
                okc += 1
    print(f"blk {blk} lba {lba} read={st} e7={st2} sn0={sn0} expect_sn={lba + 0x30000} seed={seed} edc_ok={okc}/16")
os.close(fd)
