//! SCSI デバイス抽象と Linux SG_IO 実装。
//!
//! C 実装 `libfriidump/dvd_drive.c` の `dvd_execute_cmd()` を移植する。
//! 実行層を `ScsiDevice` trait に分離し、Linux は SG_IO、将来 Windows は SPTI を実装する。
//! テストでは `MockDevice`（`#[cfg(test)]`）で置き換えられる。

use crate::drive::mmc::Command;
use crate::error::Result;

/// センスデータ（C 版 `req_sense`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sense {
    /// センスキー。
    pub sense_key: u8,
    /// ASC。
    pub asc: u8,
    /// ASCQ。
    pub ascq: u8,
}

/// コマンド実行結果（C 版 `dvd_execute_cmd` の戻り値 ＋ センス）。
#[derive(Debug, Clone, Copy, Default)]
pub struct ScsiOutcome {
    /// 失敗したかどうか（status/host/driver のいずれかにエラー）。
    pub failed: bool,
    /// センスデータ。
    pub sense: Sense,
}

impl ScsiOutcome {
    /// 成功したかどうか。
    pub fn is_ok(&self) -> bool {
        !self.failed
    }
}

/// CDB を実行できる SCSI デバイス。
pub trait ScsiDevice {
    /// コマンドを実行する。
    ///
    /// - `Direction::FromDevice`: `data` に受信データが書き込まれる。
    /// - `Direction::ToDevice`: `command.data` の内容が送信される。
    /// - 方向 NONE / 長さ 0: データ転送なし。
    fn execute(&mut self, command: &Command, data: &mut [u8]) -> Result<ScsiOutcome>;
}

/// Linux の SG_IO を使う SCSI デバイス。
#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    use crate::drive::mmc::Direction;
    use crate::error::Error;
    use std::ffi::CString;
    use std::os::raw::{c_int, c_uchar, c_uint, c_ushort, c_void};

    /// SG_IO の ioctl 番号。
    const SG_IO: libc::c_ulong = 0x2285;
    const SG_DXFER_NONE: c_int = -1;
    const SG_DXFER_TO_DEV: c_int = -2;
    const SG_DXFER_FROM_DEV: c_int = -3;

    /// Linux `scsi/sg.h` の `sg_io_hdr`。
    #[repr(C)]
    struct SgIoHdr {
        interface_id: c_int,
        dxfer_direction: c_int,
        cmd_len: c_uchar,
        mx_sb_len: c_uchar,
        iovec_count: c_ushort,
        dxfer_len: c_uint,
        dxferp: *mut c_void,
        cmdp: *mut c_uchar,
        sbp: *mut c_void,
        timeout: c_uint,
        flags: c_uint,
        pack_id: c_int,
        usr_ptr: *mut c_void,
        status: c_uchar,
        masked_status: c_uchar,
        msg_status: c_uchar,
        sb_len_wr: c_uchar,
        host_status: c_ushort,
        driver_status: c_ushort,
        resid: c_int,
        duration: c_uint,
        info: c_uint,
    }

    /// MMC コマンドのタイムアウト（ミリ秒。C 版 `MMC_CMD_TIMEOUT` は 10 秒）。
    const MMC_CMD_TIMEOUT_MS: c_uint = 10_000;

    /// `/dev/sgN` を O_RDWR で開く SG_IO デバイス。
    pub struct LinuxSg {
        fd: c_int,
    }

    impl LinuxSg {
        /// デバイスを開く（C 版 `dvd_drive_new` の open 相当）。
        pub fn open(path: &str) -> Result<Self> {
            let c = CString::new(path)
                .map_err(|_| Error::InvalidArgument(format!("不正なデバイスパス: {path}")))?;
            // Linux の SG_IO はブロックデバイス(/dev/srN)ではベンダコマンド 0xe7 を
            // 拒否するため、文字デバイス(/dev/sgN)を O_RDWR で開く。
            let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
            if fd < 0 {
                return Err(Error::Io(std::io::Error::last_os_error()));
            }
            Ok(Self { fd })
        }
    }

    impl Drop for LinuxSg {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.fd);
            }
        }
    }

    impl ScsiDevice for LinuxSg {
        fn execute(&mut self, command: &Command, data: &mut [u8]) -> Result<ScsiOutcome> {
            let mut cdb = command.cdb;
            let mut sense = [0u8; 32];

            let (direction, dxferp, dxfer_len) = match command.direction {
                Direction::FromDevice if !data.is_empty() => (
                    SG_DXFER_FROM_DEV,
                    data.as_mut_ptr() as *mut c_void,
                    data.len() as c_uint,
                ),
                Direction::ToDevice if !command.data.is_empty() => (
                    SG_DXFER_TO_DEV,
                    command.data.as_ptr() as *mut c_void,
                    command.data.len() as c_uint,
                ),
                _ => (SG_DXFER_NONE, std::ptr::null_mut(), 0),
            };

            let mut hdr = SgIoHdr {
                interface_id: 'S' as c_int,
                dxfer_direction: direction,
                cmd_len: 12,
                mx_sb_len: sense.len() as c_uchar,
                iovec_count: 0,
                dxfer_len,
                dxferp,
                cmdp: cdb.as_mut_ptr(),
                sbp: sense.as_mut_ptr() as *mut c_void,
                timeout: MMC_CMD_TIMEOUT_MS,
                flags: 0,
                pack_id: 0,
                usr_ptr: std::ptr::null_mut(),
                status: 0,
                masked_status: 0,
                msg_status: 0,
                sb_len_wr: 0,
                host_status: 0,
                driver_status: 0,
                resid: 0,
                duration: 0,
                info: 0,
            };

            let rc = unsafe { libc::ioctl(self.fd, SG_IO, &mut hdr as *mut SgIoHdr) };
            if rc < 0 {
                return Err(Error::Io(std::io::Error::last_os_error()));
            }

            let failed =
                ((hdr.status & 0x7e) != 0) || hdr.host_status != 0 || hdr.driver_status != 0;
            let outcome = ScsiOutcome {
                failed,
                sense: Sense {
                    sense_key: sense[2] & 0x0f,
                    asc: sense[12],
                    ascq: sense[13],
                },
            };
            Ok(outcome)
        }
    }
}
