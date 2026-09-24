//! DVD ドライブの高レベルラッパー。
//!
//! 元とした C 実装相当。`ScsiDevice` にコマンドを発行し、
//! INQUIRY で機種を判定して特性（memdump 種別・E7 ベース・既定 method）を割り当てる。

use crate::drive::device::{ScsiDevice, ScsiOutcome};
use crate::drive::mmc;
use crate::drive::profile::{lookup, DriveProfile, MemdumpKind, ReadFamily};
use crate::error::{Error, Result};

/// Hitachi MN103S（GCC-4160N/4240N）の E7 ベースアドレス。
pub const MN103S_MEM_BASE: u32 = 0xA13000;
/// 従来 Hitachi MN103 系の E7 ベースアドレス。
pub const HITACHI_MEM_BASE: u32 = 0x80000000;

/// DVD ドライブ。
pub struct DvdDrive<D: ScsiDevice> {
    /// コマンド実行先。
    pub device: D,
    /// INQUIRY のベンダー。
    pub vendor: String,
    /// INQUIRY の製品 ID。
    pub prod_id: String,
    /// INQUIRY のリビジョン。
    pub prod_rev: String,
    /// `vendor/prod_id/prod_rev` を結合した表示用文字列。
    pub model: String,
    /// memdump 実装種別。
    pub memdump_kind: MemdumpKind,
    /// E7 のベースアドレス。
    pub mem_base: u32,
    /// 読み出しファミリ。
    pub family: ReadFamily,
    /// 既定 method。
    pub def_method: u32,
    /// CLI `-c` で強制されたコマンド ID（未指定は -1）。
    pub command: i64,
    /// 対応ドライブかどうか。
    pub supported: bool,
}

impl<D: ScsiDevice> DvdDrive<D> {
    /// ドライブを生成する（INQUIRY は別途 `inquiry`）。
    pub fn new(device: D, command: i64) -> Self {
        Self {
            device,
            vendor: String::new(),
            prod_id: String::new(),
            prod_rev: String::new(),
            model: String::new(),
            memdump_kind: MemdumpKind::Vanilla2064,
            mem_base: 0,
            family: ReadFamily::Default,
            def_method: 0,
            command,
            supported: false,
        }
    }

    /// INQUIRY を発行して機種情報を取得し、特性を割り当てる（C 版 `dvd_get_drive_info` + `dvd_assign_functions`）。
    pub fn inquiry(&mut self) -> Result<()> {
        let cmd = mmc::inquiry(36);
        let mut buf = [0u8; 36];
        let out = self.device.execute(&cmd, &mut buf)?;
        if out.failed {
            return Err(Error::Other("INQUIRY に失敗しました".to_string()));
        }
        self.vendor = trim_ascii(&buf[8..16]);
        self.prod_id = trim_ascii(&buf[16..32]);
        self.prod_rev = trim_ascii(&buf[32..36]);
        self.model = format!("{}/{}/{}", self.vendor, self.prod_id, self.prod_rev);
        self.assign_functions();
        Ok(())
    }

    /// 機種情報から特性を割り当てる（C 版 `dvd_assign_functions`）。
    fn assign_functions(&mut self) {
        // 未対応ドライブの既定値
        self.def_method = 0;
        self.mem_base = 0;
        self.family = ReadFamily::Default;
        self.memdump_kind = MemdumpKind::Vanilla2064;
        self.command = if self.command == -1 { -1 } else { self.command };
        self.supported = false;

        let matched: Option<&'static DriveProfile> = lookup(&self.vendor, &self.prod_id);
        if let Some(p) = matched {
            self.memdump_kind = p.memdump;
            self.mem_base = p.mem_base;
            self.family = p.family;
            self.def_method = p.def_method;
            self.supported = true;
        }

        if self.command != -1 {
            self.memdump_kind = match self.command {
                0 => MemdumpKind::Vanilla2064,
                1 => MemdumpKind::Vanilla2384,
                2 => MemdumpKind::Hitachi,
                3 => MemdumpKind::LiteOn,
                4 => MemdumpKind::Renesas,
                _ => self.memdump_kind,
            };
        }
    }

    /// READ(12) を発行する（センス付き。C 版 `dvd_read_sector_dummy`）。
    ///
    /// C 版と同様に常に受信用バッファを渡す（intbuf 64KB 相当）。空バッファ
    /// （`dxfer_len=0` / `SG_DXFER_NONE`）では READ が実行されずセンスが得られないため、
    /// ディスク種別の自動判定が機能しない。
    pub fn read_sector_dummy(&mut self, sector: u32, sectors: u32) -> Result<ScsiOutcome> {
        let cmd = mmc::read12_dummy(sector, sectors);
        let mut buf = vec![0u8; 64 * 1024];
        self.device.execute(&cmd, &mut buf)
    }

    /// READ(12) streaming を発行する（C 版 `dvd_read_sector_streaming`）。
    ///
    /// C 版は `ignore_errors=true` のためコマンド失敗は無視する（デシンク検出は E7 の
    /// セクタ番号照合で行う）。ここでもコマンド成否は無視し、転送エラーのみ伝播する。
    pub fn read_sector_streaming(&mut self, sector: u32, buf: &mut [u8]) -> Result<()> {
        let cmd = mmc::read12_streaming(sector);
        let _ = self.device.execute(&cmd, buf)?;
        Ok(())
    }

    /// READ(12) streaming（データ破棄）を発行する。
    pub fn read_sector_streaming_discard(&mut self, sector: u32) -> Result<()> {
        let mut buf = [0u8; 2048 * 16];
        self.read_sector_streaming(sector, &mut buf)
    }

    /// READ(12) を任意セクタ数で発行する（C 版 `dvd_read_streaming`）。
    pub fn read_streaming(&mut self, sector: u32, sectors: u32, buf: &mut [u8]) -> Result<()> {
        let cmd = mmc::read12_streaming_n(sector, sectors, buf.len());
        let _ = self.device.execute(&cmd, buf)?;
        Ok(())
    }

    /// キャッシュを流す（C 版 `dvd_flush_cache_READ12`）。
    pub fn flush_cache_read12(&mut self, sector: u32) -> Result<()> {
        let cmd = mmc::flush_cache_read12(sector);
        let _ = self.device.execute(&cmd, &mut [])?;
        Ok(())
    }

    /// 回転開始/停止（C 版 `dvd_stop_unit`）。
    pub fn stop_unit(&mut self, start: bool) -> Result<()> {
        let cmd = mmc::stop_unit(start);
        let _ = self.device.execute(&cmd, &mut [])?;
        Ok(())
    }

    /// 速度設定（C 版 `dvd_set_speed`）。
    pub fn set_speed(&mut self, speed: u32) -> Result<()> {
        let cmd = mmc::set_speed(speed);
        let _ = self.device.execute(&cmd, &mut [])?;
        Ok(())
    }

    /// streaming 速度設定（C 版 `dvd_set_streaming`）。
    pub fn set_streaming(&mut self, speed: u32) -> Result<()> {
        let cmd = mmc::set_streaming(speed);
        let _ = self.device.execute(&cmd, &mut [])?;
        Ok(())
    }

    /// 総セクタ数を取得する（C 版 `dvd_get_size`）。
    pub fn get_size(&mut self) -> Result<u32> {
        let cmd = mmc::read_capacity();
        let mut buf = [0u8; 0x22];
        let _ = self.device.execute(&cmd, &mut buf)?;
        Ok(mmc::parse_read_capacity(&buf))
    }

    /// layer break を取得する（C 版 `dvd_get_layerbreak`）。
    pub fn get_layerbreak(&mut self) -> Result<u32> {
        let cmd = mmc::read_dvd_structure();
        let mut buf = [0u8; 2052];
        let _ = self.device.execute(&cmd, &mut buf)?;
        Ok(mmc::parse_layerbreak(&buf))
    }

    /// ドライブ内メモリをダンプする（C 版 `dvd_memdump`）。
    pub fn memdump(
        &mut self,
        offset: u32,
        block_len: u32,
        block_size: u32,
        buf: &mut [u8],
    ) -> Result<()> {
        match self.memdump_kind {
            MemdumpKind::HitachiMn103s => {
                self.e7_memdump_loop(MN103S_MEM_BASE, offset, block_len, block_size, buf)
            }
            MemdumpKind::Hitachi => {
                self.e7_memdump_loop(HITACHI_MEM_BASE, offset, block_len, block_size, buf)
            }
            MemdumpKind::Vanilla2064 => {
                self.read_buffer_loop(offset, block_len, block_size, buf, false)
            }
            MemdumpKind::Vanilla2384 => Err(Error::Unsupported(
                "vanilla 2384 の memdump は未実装です".to_string(),
            )),
            MemdumpKind::LiteOn => Err(Error::Unsupported(
                "Lite-On の memdump は未実装です".to_string(),
            )),
            MemdumpKind::Renesas => Err(Error::Unsupported(
                "Renesas の memdump は未実装です".to_string(),
            )),
        }
    }

    fn e7_memdump_loop(
        &mut self,
        base: u32,
        offset: u32,
        block_len: u32,
        block_size: u32,
        buf: &mut [u8],
    ) -> Result<()> {
        for i in 0..block_len {
            let addr = base
                .wrapping_add(offset)
                .wrapping_add(i.wrapping_mul(block_size));
            let cmd = mmc::e7_memdump(addr, block_size as u16);
            let start = (i * block_size) as usize;
            let end = start + block_size as usize;
            if end > buf.len() {
                return Err(Error::InvalidArgument(
                    "memdump バッファ長が不足しています".to_string(),
                ));
            }
            let out = self.device.execute(&cmd, &mut buf[start..end])?;
            if out.failed {
                return Err(Error::Other(format!(
                    "E7 memdump 失敗: offset={addr:#010x}"
                )));
            }
        }
        Ok(())
    }

    fn read_buffer_loop(
        &mut self,
        offset: u32,
        block_len: u32,
        block_size: u32,
        buf: &mut [u8],
        _liteon: bool,
    ) -> Result<()> {
        for i in 0..block_len {
            let off = offset.wrapping_add(i.wrapping_mul(block_size));
            let cmd = mmc::read_buffer_vanilla2064(off, block_size);
            let start = (i * block_size) as usize;
            let end = start + block_size as usize;
            if end > buf.len() {
                return Err(Error::InvalidArgument(
                    "memdump バッファ長が不足しています".to_string(),
                ));
            }
            let out = self.device.execute(&cmd, &mut buf[start..end])?;
            if out.failed {
                return Err(Error::Other(format!(
                    "0x3C memdump 失敗: offset={off:#010x}"
                )));
            }
        }
        Ok(())
    }
}

/// ASCII バイト列の前後空白を除去して文字列化する（C 版 `strtrimr`）。
fn trim_ascii(bytes: &[u8]) -> String {
    let s = bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                ' '
            }
        })
        .collect::<String>();
    s.trim().to_string()
}

/// プラットフォーム既定の SCSI デバイス。
#[cfg(target_os = "linux")]
pub type PlatformDevice = crate::drive::device::linux::LinuxSg;

#[cfg(target_os = "linux")]
impl DvdDrive<PlatformDevice> {
    /// デバイスを開いて INQUIRY まで行う（C 版 `dvd_drive_new`）。
    pub fn open(path: &str, command: i64) -> Result<Self> {
        let device = PlatformDevice::open(path)?;
        let mut drive = Self::new(device, command);
        drive.inquiry()?;
        Ok(drive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::device::Sense;
    use crate::drive::mmc::Command;
    use std::collections::VecDeque;

    /// 応答をキューで返すモックデバイス。
    struct MockDevice {
        /// 実行ごとに返す結果（先頭から消費）。
        outcomes: VecDeque<(bool, Vec<u8>)>,
        /// 実行された CDB を記録。
        pub seen: Vec<[u8; 12]>,
        /// 直近の execute に渡されたデータバッファ長。
        pub last_data_len: usize,
    }

    impl MockDevice {
        fn new(responses: Vec<(bool, Vec<u8>)>) -> Self {
            Self {
                outcomes: responses.into(),
                seen: Vec::new(),
                last_data_len: 0,
            }
        }
    }

    impl ScsiDevice for MockDevice {
        fn execute(&mut self, command: &Command, data: &mut [u8]) -> Result<ScsiOutcome> {
            self.seen.push(command.cdb);
            self.last_data_len = data.len();
            let (failed, resp) = self.outcomes.pop_front().unwrap_or((false, Vec::new()));
            let n = resp.len().min(data.len());
            data[..n].copy_from_slice(&resp[..n]);
            Ok(ScsiOutcome {
                failed,
                sense: Sense::default(),
            })
        }
    }

    fn inquiry_response(vendor: &str, prod: &str, rev: &str) -> Vec<u8> {
        let mut b = vec![0u8; 36];
        b[8..8 + vendor.len()].copy_from_slice(vendor.as_bytes());
        b[16..16 + prod.len()].copy_from_slice(prod.as_bytes());
        b[32..32 + rev.len()].copy_from_slice(rev.as_bytes());
        b
    }

    #[test]
    fn inquiry_matches_gcc4240n_profile() {
        let m = MockDevice::new(vec![(
            false,
            inquiry_response("HL-DT-ST", "RW/DVD GCC-4240N", "E112"),
        )]);
        let mut d = DvdDrive::new(m, -1);
        d.inquiry().unwrap();
        assert_eq!(d.vendor, "HL-DT-ST");
        assert_eq!(d.prod_id, "RW/DVD GCC-4240N");
        assert_eq!(d.memdump_kind, MemdumpKind::HitachiMn103s);
        assert_eq!(d.mem_base, MN103S_MEM_BASE);
        assert_eq!(d.def_method, 12);
        assert!(d.supported);
    }

    #[test]
    fn command_override_changes_memdump() {
        let m = MockDevice::new(vec![(
            false,
            inquiry_response("HL-DT-ST", "GCC-4240N", "E112"),
        )]);
        let mut d = DvdDrive::new(m, 0);
        d.inquiry().unwrap();
        assert_eq!(d.memdump_kind, MemdumpKind::Vanilla2064);
    }

    #[test]
    fn memdump_sends_e7_with_base() {
        // 2 ブロック分の応答
        let resp = vec![0xABu8; 33024];
        let m = MockDevice::new(vec![(false, resp.clone()), (false, resp)]);
        let mut d = DvdDrive::new(m, -1);
        // 手動で HitachiMn103s を設定
        d.memdump_kind = MemdumpKind::HitachiMn103s;
        let mut buf = vec![0u8; 33024 * 2];
        d.memdump(0, 2, 33024, &mut buf).unwrap();
        assert_eq!(&buf[..33024], &vec![0xABu8; 33024][..]);
        // 1 命令目は base + 0
        let d0 = d.device.seen[0];
        assert_eq!(d0[0], 0xE7);
        assert_eq!(&d0[6..10], &[0x00, 0xA1, 0x30, 0x00]);
        // 2 命令目は base + 33024 = 0xA1B100
        let d1 = d.device.seen[1];
        assert_eq!(&d1[6..10], &[0x00, 0xA1, 0xB1, 0x00]);
    }

    #[test]
    fn dummy_read_passes_data_buffer() {
        // 範囲外 READ は CHECK CONDITION を返す想定（failed=true）
        let m = MockDevice::new(vec![(true, Vec::new())]);
        let mut d = DvdDrive::new(m, -1);
        let out = d.read_sector_dummy(712_980, 16).unwrap();
        assert!(out.failed);
        let cdb = d.device.seen[0];
        assert_eq!(cdb[0], 0xA8); // READ(12)
        assert_eq!(cdb[1], 0x08); // FUA
        assert_eq!(&cdb[6..10], &[0x00, 0x00, 0x00, 0x10]); // 16 セクタ
                                                            // 空バッファ（dxfer_len=0）では READ が実行されないため、十分なバッファを渡すこと
        assert!(d.device.last_data_len >= 16 * 2048);
    }
}
