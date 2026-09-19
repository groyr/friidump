#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
USBリンク速度プローブ（Windows）

SetupAPI で全 USB ハブを列挙し、各ポートに接続されたデバイスの
接続速度（Low/Full/High/Super）を IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX
で取得する。VID/PID を指定すると一致するデバイスだけを表示する。

光学ドライブのブリッジが Full-Speed(12Mbps) に落ちていないかの確認用。
"""

import ctypes
import sys
from ctypes import wintypes

# --- 定数 ---
GUID_DEVINTERFACE_USB_HUB = "{F18A0E88-C30C-11D0-8815-00A0C906BED8}"

DIGCF_PRESENT = 0x02
DIGCF_DEVICEINTERFACE = 0x10
ERROR_INSUFFICIENT_BUFFER = 122

IOCTL_USB_GET_NODE_INFORMATION = 0x00220408
IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX = 0x00220448

GENERIC_WRITE = 0x40000000
FILE_SHARE_READ = 0x01
FILE_SHARE_WRITE = 0x02
OPEN_EXISTING = 3
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value

# USB_SPEED_*
SPEED_NAMES = {
    0: "Low-Speed (1.5 Mbps)",
    1: "Full-Speed (12 Mbps)",
    2: "High-Speed (480 Mbps)",
    3: "SuperSpeed (5 Gbps)",
}


class GUID(ctypes.Structure):
    _fields_ = [
        ("Data1", ctypes.c_ulong),
        ("Data2", ctypes.c_ushort),
        ("Data3", ctypes.c_ushort),
        ("Data4", ctypes.c_ubyte * 8),
    ]


def guid_from_string(s):
    import uuid
    u = uuid.UUID(s)
    g = GUID()
    g.Data1 = u.time_low
    g.Data2 = u.time_mid
    g.Data3 = u.time_hi_version
    g.Data4 = (ctypes.c_ubyte * 8)(*u.bytes[8:])
    return g


class SP_DEVICE_INTERFACE_DATA(ctypes.Structure):
    _fields_ = [
        ("cbSize", wintypes.DWORD),
        ("InterfaceClassGuid", GUID),
        ("Flags", wintypes.DWORD),
        ("Reserved", ctypes.POINTER(ctypes.c_ulong)),
    ]


class USB_DEVICE_DESCRIPTOR(ctypes.Structure):
    _pack_ = 1
    _fields_ = [
        ("bLength", ctypes.c_ubyte),
        ("bDescriptorType", ctypes.c_ubyte),
        ("bcdUSB", ctypes.c_ushort),
        ("bDeviceClass", ctypes.c_ubyte),
        ("bDeviceSubClass", ctypes.c_ubyte),
        ("bDeviceProtocol", ctypes.c_ubyte),
        ("bMaxPacketSize0", ctypes.c_ubyte),
        ("idVendor", ctypes.c_ushort),
        ("idProduct", ctypes.c_ushort),
        ("bcdDevice", ctypes.c_ushort),
        ("iManufacturer", ctypes.c_ubyte),
        ("iProduct", ctypes.c_ubyte),
        ("iSerialNumber", ctypes.c_ubyte),
        ("bNumConfigurations", ctypes.c_ubyte),
    ]


class USB_NODE_CONNECTION_INFORMATION_EX(ctypes.Structure):
    _fields_ = [
        ("ConnectionIndex", ctypes.c_ulong),
        ("DeviceDescriptor", USB_DEVICE_DESCRIPTOR),
        ("CurrentConfigurationValue", ctypes.c_ubyte),
        ("Speed", ctypes.c_ubyte),
        ("DeviceIsHub", ctypes.c_ubyte),
        ("DeviceAddress", ctypes.c_ushort),
        ("NumberOfOpenPipes", ctypes.c_ulong),
    ]


setupapi = ctypes.WinDLL("setupapi", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

setupapi.SetupDiGetClassDevsW.restype = ctypes.c_void_p
setupapi.SetupDiGetClassDevsW.argtypes = [
    ctypes.POINTER(GUID), wintypes.LPCWSTR, wintypes.HWND, wintypes.DWORD,
]
setupapi.SetupDiEnumDeviceInterfaces.restype = wintypes.BOOL
setupapi.SetupDiEnumDeviceInterfaces.argtypes = [
    ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(GUID), wintypes.DWORD,
    ctypes.POINTER(SP_DEVICE_INTERFACE_DATA),
]
setupapi.SetupDiGetDeviceInterfaceDetailW.restype = wintypes.BOOL
setupapi.SetupDiGetDeviceInterfaceDetailW.argtypes = [
    ctypes.c_void_p, ctypes.POINTER(SP_DEVICE_INTERFACE_DATA),
    ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p,
]
setupapi.SetupDiDestroyDeviceInfoList.restype = wintypes.BOOL
setupapi.SetupDiDestroyDeviceInfoList.argtypes = [ctypes.c_void_p]

kernel32.CreateFileW.restype = wintypes.HANDLE
kernel32.CreateFileW.argtypes = [
    wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
    wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE,
]
kernel32.DeviceIoControl.restype = wintypes.BOOL
kernel32.DeviceIoControl.argtypes = [
    wintypes.HANDLE, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD,
    ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p,
]
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]


def get_hub_paths():
    """インターフェースパスを持つ USB ハブの一覧を返す"""
    guid = guid_from_string(GUID_DEVINTERFACE_USB_HUB)
    hdev = setupapi.SetupDiGetClassDevsW(
        ctypes.byref(guid), None, None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE
    )
    if hdev == ctypes.c_void_p(-1).value or not hdev:
        return []
    paths = []
    idx = 0
    while True:
        did = SP_DEVICE_INTERFACE_DATA()
        did.cbSize = ctypes.sizeof(SP_DEVICE_INTERFACE_DATA)
        if not setupapi.SetupDiEnumDeviceInterfaces(hdev, None, ctypes.byref(guid), idx, ctypes.byref(did)):
            break
        req = wintypes.DWORD(0)
        setupapi.SetupDiGetDeviceInterfaceDetailW(hdev, ctypes.byref(did), None, 0, ctypes.byref(req), None)
        buf = ctypes.create_string_buffer(req.value)
        # cbSize は 64bit では 8、32bit では 6
        detail = ctypes.cast(buf, ctypes.POINTER(wintypes.DWORD))
        detail[0] = 8 if ctypes.sizeof(ctypes.c_void_p) == 8 else 6
        if setupapi.SetupDiGetDeviceInterfaceDetailW(hdev, ctypes.byref(did), buf, req.value, ctypes.byref(req), None):
            path = ctypes.wstring_at(ctypes.addressof(buf) + ctypes.sizeof(wintypes.DWORD))
            paths.append(path)
        idx += 1
    setupapi.SetupDiDestroyDeviceInfoList(hdev)
    return paths


def open_device(path):
    h = kernel32.CreateFileW(
        path, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, None,
        OPEN_EXISTING, 0, None,
    )
    if h == INVALID_HANDLE_VALUE:
        return None
    return h


def num_ports(handle):
    # IOCTL_USB_GET_NODE_INFORMATION: NodeType(4) + HubDescriptor の bNumberOfPorts は offset 6
    buf = ctypes.create_string_buffer(1024)
    ret = wintypes.DWORD(0)
    if not kernel32.DeviceIoControl(handle, IOCTL_USB_GET_NODE_INFORMATION, None, 0, buf, 1024, ctypes.byref(ret), None):
        return 0
    return ctypes.cast(ctypes.addressof(buf) + 6, ctypes.POINTER(ctypes.c_ubyte))[0]


def port_info(handle, port):
    # パイプ情報 (USB_PIPE_INFO 12B × N) を含められるよう大きめのバッファを確保する。
    # 32B ちょうどだとパイプがある場合に ERROR_INSUFFICIENT_BUFFER で失敗する。
    bufsize = 32 + 12 * 32
    buf = ctypes.create_string_buffer(bufsize)
    info = ctypes.cast(buf, ctypes.POINTER(USB_NODE_CONNECTION_INFORMATION_EX)).contents
    info.ConnectionIndex = port
    ret = wintypes.DWORD(0)
    ok = kernel32.DeviceIoControl(
        handle, IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX,
        ctypes.byref(info), ctypes.sizeof(USB_NODE_CONNECTION_INFORMATION_EX),
        buf, bufsize, ctypes.byref(ret), None,
    )
    return info if ok else None


def main():
    want = None
    if len(sys.argv) > 1 and ":" in sys.argv[1]:
        v, p = sys.argv[1].split(":")
        want = (int(v, 16), int(p, 16))

    hubs = get_hub_paths()
    print("USBハブ数: %d" % len(hubs))
    found = []
    for hp in hubs:
        h = open_device(hp)
        if h is None:
            err = ctypes.get_last_error()
            print("  ハブを開けません (err=%d): 管理者権限が必要な可能性: %s" % (err, hp))
            continue
        try:
            n = num_ports(h)
            print("  ハブ %s ... %d ポート" % (hp, n))
            for port in range(1, n + 1):
                info = port_info(h, port)
                if info is None or info.DeviceDescriptor.idVendor == 0:
                    continue
                vid = info.DeviceDescriptor.idVendor
                pid = info.DeviceDescriptor.idProduct
                speed = SPEED_NAMES.get(info.Speed, "Unknown(%d)" % info.Speed)
                line = "VID_%04X PID_%04X  port=%d  %s" % (vid, pid, port, speed)
                found.append((vid, pid, info.Speed, line))
                if want is None or want == (vid, pid):
                    print("  " + line)
        finally:
            kernel32.CloseHandle(h)

    if want is not None:
        hits = [f for f in found if (f[0], f[1]) == want]
        if not hits:
            print("VID_%04X:PID_%04X は見つかりませんでした" % want)
        elif hits[0][2] < 2:
            print("\n警告: このデバイスは High-Speed 未満です。")
            print("ケーブル/ポート交換、または別ブリッジで大幅高速化の可能性があります。")
        else:
            print("\nHigh-Speed 以上で接続されています。")


if __name__ == "__main__":
    main()
