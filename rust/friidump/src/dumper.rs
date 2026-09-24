//! 吸い出し本体（raw/iso 出力・ジャーナル・resume・進捗）。
//!
//! C 実装 `libfriidump/dumper.c` を移植する。出力は `BufWriter` でまとめ書きし、
//! ジャーナル（`<base>.journal` の `next=<sector>`）でクラッシュ耐性を持たせる。
//!
//! `-t/--startsector`（強制開始）指定時は読み出しをそこから、出力を先頭(0)から書く。
//! `-e/--stopsector` は **このセクタを含まない**（exclusive。help 表記に合わせる）。

use crate::constants::{RAW_SECTOR_SIZE, SECTORS_PER_BLOCK, SECTOR_SIZE};
use crate::disc::Disc;
use crate::drive::device::ScsiDevice;
use crate::error::{Error, Result};
use crate::hasher::{Digests, MultiHash};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// ジャーナルを書き込む間隔（セクタ）。
const JOURNAL_INTERVAL: u32 = 8192;
/// 進捗更新・flush の区切り（セクタ）。
const PROGRESS_INTERVAL: u32 = 320;
/// 出力バッファ長。
const OUTPUT_BUFFER_SIZE: usize = 1024 * 1024;

/// 進捗コールバック `(start, sectors_done, total_sectors)`。
pub type ProgressFn<'a> = Box<dyn FnMut(bool, u32, u32) + 'a>;

/// 出力指定（prepare 前）。
struct Pending {
    path: PathBuf,
    start_sector: u32,
}

/// 吸い出し器。
pub struct Dumper<'a, D: ScsiDevice> {
    disc: &'a mut Disc<D>,
    raw_pending: Option<Pending>,
    iso_pending: Option<Pending>,
    raw: Option<BufWriter<File>>,
    iso: Option<BufWriter<File>>,
    journal: Option<File>,
    hash_raw: Option<MultiHash>,
    hash_iso: Option<MultiHash>,
    hash_raw_done: Option<Digests>,
    hash_iso_done: Option<Digests>,
    hashing: bool,
    flushing: bool,
    /// 明示指定の開始セクタ（`-t`、未指定は None）。
    forced_start: Option<u32>,
    /// 明示指定の終了セクタ（`-e`、exclusive。未指定は None）。
    forced_end: Option<u32>,
    start_sector: u32,
    write_start_sector: u32,
    progress: Option<ProgressFn<'a>>,
}

impl<'a, D: ScsiDevice> Dumper<'a, D> {
    /// 生成する。
    pub fn new(disc: &'a mut Disc<D>) -> Self {
        Self {
            disc,
            raw_pending: None,
            iso_pending: None,
            raw: None,
            iso: None,
            journal: None,
            hash_raw: None,
            hash_iso: None,
            hash_raw_done: None,
            hash_iso_done: None,
            hashing: true,
            flushing: true,
            forced_start: None,
            forced_end: None,
            start_sector: 0,
            write_start_sector: 0,
            progress: None,
        }
    }

    /// ハッシュ計算の有無。
    pub fn set_hashing(&mut self, on: bool) {
        self.hashing = on;
    }

    /// 出力 flush の有無。
    pub fn set_flushing(&mut self, on: bool) {
        self.flushing = on;
    }

    /// 強制開始セクタ（`-t`）。
    pub fn set_start_sector(&mut self, s: u32) {
        self.forced_start = Some(s);
    }

    /// 強制終了セクタ（`-e`、exclusive）。
    pub fn set_end_sector(&mut self, e: u32) {
        self.forced_end = Some(e);
    }

    /// 進捗コールバックを設定する。
    pub fn set_progress(&mut self, f: ProgressFn<'a>) {
        self.progress = Some(f);
    }

    /// raw 出力ファイルを指定する。
    pub fn set_raw_output(&mut self, path: &Path, resume: bool) -> Result<()> {
        self.raw_pending = Some(pending_for(path, RAW_SECTOR_SIZE, resume, "raw")?);
        Ok(())
    }

    /// ISO 出力ファイルを指定する。
    pub fn set_iso_output(&mut self, path: &Path, resume: bool) -> Result<()> {
        self.iso_pending = Some(pending_for(path, SECTOR_SIZE, resume, "ISO")?);
        Ok(())
    }

    /// 準備（開始位置・ジャーナル・切り詰め・既存分ハッシュ）（C 版 `dumper_prepare`）。
    pub fn prepare(&mut self, resume: bool) -> Result<()> {
        self.start_sector = match (&self.raw_pending, &self.iso_pending) {
            (Some(r), Some(i)) => r.start_sector.min(i.start_sector),
            (Some(r), None) => r.start_sector,
            (None, Some(i)) => i.start_sector,
            (None, None) => {
                return Err(Error::InvalidArgument(
                    "出力ファイルが未設定です".to_string(),
                ))
            }
        };

        if let Some(fs) = self.forced_start {
            let spb = SECTORS_PER_BLOCK as u32;
            self.start_sector = (fs / spb) * spb;
            self.write_start_sector = 0;
        } else {
            self.write_start_sector = self.start_sector;
        }

        // ジャーナル（raw 優先）
        if let Some(base) = self
            .raw_pending
            .as_ref()
            .map(|r| r.path.clone())
            .or_else(|| self.iso_pending.as_ref().map(|i| i.path.clone()))
        {
            let mut p = base.into_os_string();
            p.push(".journal");
            let path = PathBuf::from(p);
            if resume {
                if let Some(j) = read_journal(&path) {
                    if j > 0 && j < self.start_sector {
                        let spb = SECTORS_PER_BLOCK as u32;
                        self.start_sector = (j / spb) * spb;
                    }
                }
            }
            let f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;
            self.journal = Some(f);
        }

        if self.hashing {
            self.hash_raw = Some(MultiHash::new());
            self.hash_iso = Some(MultiHash::new());
        }

        self.open_outputs(resume)?;

        if let Some(j) = self.journal.as_mut() {
            write_journal(j, self.start_sector)?;
        }
        Ok(())
    }

    fn open_outputs(&mut self, resume: bool) -> Result<()> {
        let start = self.start_sector;
        let write_start = self.write_start_sector;
        let hash_existing = self.hashing && self.forced_start.is_none() && resume && start > 0;

        if let Some(p) = self.raw_pending.take() {
            let mut f = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&p.path)?;
            if hash_existing {
                f.seek(SeekFrom::Start(0))?;
                let mut buf = [0u8; RAW_SECTOR_SIZE];
                for _ in 0..start {
                    if f.read(&mut buf)? < RAW_SECTOR_SIZE {
                        break;
                    }
                    if let Some(h) = self.hash_raw.as_mut() {
                        h.update(&buf);
                    }
                }
            }
            f.set_len(write_start as u64 * RAW_SECTOR_SIZE as u64)?;
            f.seek(SeekFrom::Start(write_start as u64 * RAW_SECTOR_SIZE as u64))?;
            self.raw = Some(BufWriter::with_capacity(OUTPUT_BUFFER_SIZE, f));
        }

        if let Some(p) = self.iso_pending.take() {
            let mut f = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&p.path)?;
            if hash_existing {
                f.seek(SeekFrom::Start(0))?;
                let mut buf = [0u8; SECTOR_SIZE];
                for _ in 0..start {
                    if f.read(&mut buf)? < SECTOR_SIZE {
                        break;
                    }
                    if let Some(h) = self.hash_iso.as_mut() {
                        h.update(&buf);
                    }
                }
            }
            f.set_len(write_start as u64 * SECTOR_SIZE as u64)?;
            f.seek(SeekFrom::Start(write_start as u64 * SECTOR_SIZE as u64))?;
            self.iso = Some(BufWriter::with_capacity(OUTPUT_BUFFER_SIZE, f));
        }
        Ok(())
    }

    /// 吸い出しを実行する（C 版 `dumper_dump`）。
    // C 版の `i % PROGRESS_INTERVAL` 等をそのまま維持する（古い rustc でも動くよう is_multiple_of は使わない）
    #[allow(clippy::manual_is_multiple_of)]
    pub fn dump(&mut self) -> Result<()> {
        let sectors_no = self.disc.sectors_no;
        if sectors_no == 0 {
            return Err(Error::Other("総セクタ数が不明です".to_string()));
        }
        let last = match self.forced_end {
            Some(e) if e <= sectors_no => e.saturating_sub(1),
            _ => sectors_no - 1,
        };

        if let Some(p) = self.progress.as_mut() {
            p(true, self.start_sector, sectors_no);
        }

        let mut iso_sector = [0u8; SECTOR_SIZE];
        let mut raw_sector = [0u8; RAW_SECTOR_SIZE];
        for i in self.start_sector..=last {
            self.disc
                .read_sector(i)
                .map_err(|e| Error::Other(format!("sector {i} の読み出しに失敗: {e}")))?;
            {
                let (d, r) = self
                    .disc
                    .sector(i)
                    .ok_or_else(|| Error::Other(format!("sector {i} がキャッシュにありません")))?;
                let w = (i % SECTORS_PER_BLOCK as u32) as usize;
                iso_sector.copy_from_slice(&d[w * SECTOR_SIZE..w * SECTOR_SIZE + SECTOR_SIZE]);
                raw_sector.copy_from_slice(
                    &r[w * RAW_SECTOR_SIZE..w * RAW_SECTOR_SIZE + RAW_SECTOR_SIZE],
                );
            }

            if let Some(f) = self.raw.as_mut() {
                f.write_all(&raw_sector)?;
                if let Some(h) = self.hash_raw.as_mut() {
                    h.update(&raw_sector);
                }
            }
            if let Some(f) = self.iso.as_mut() {
                f.write_all(&iso_sector)?;
                if let Some(h) = self.hash_iso.as_mut() {
                    h.update(&iso_sector);
                }
            }

            if (i % PROGRESS_INTERVAL == 0) || i == last {
                if self.flushing {
                    if let Some(f) = self.raw.as_mut() {
                        f.flush()?;
                    }
                    if let Some(f) = self.iso.as_mut() {
                        f.flush()?;
                    }
                }
                if let Some(p) = self.progress.as_mut() {
                    p(false, i + 1, sectors_no);
                }
            }

            if let Some(j) = self.journal.as_mut() {
                if ((i + 1) % JOURNAL_INTERVAL == 0) || i == last {
                    write_journal(j, i + 1)?;
                }
            }
        }

        if let Some(f) = self.raw.as_mut() {
            f.flush()?;
        }
        if let Some(f) = self.iso.as_mut() {
            f.flush()?;
        }
        if let Some(h) = self.hash_raw.take() {
            self.hash_raw_done = Some(h.finish());
        }
        if let Some(h) = self.hash_iso.take() {
            self.hash_iso_done = Some(h.finish());
        }
        Ok(())
    }

    /// raw のハッシュ（dump 完了後のみ）。
    pub fn raw_digests(&self) -> Option<&Digests> {
        self.hash_raw_done.as_ref()
    }

    /// ISO のハッシュ（dump 完了後のみ）。
    pub fn iso_digests(&self) -> Option<&Digests> {
        self.hash_iso_done.as_ref()
    }
}

/// 既存ファイルのサイズから開始セクタを求める（resume 未指定で存在すればエラー）。
fn pending_for(path: &Path, sector_size: usize, resume: bool, kind: &str) -> Result<Pending> {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > 0 && !resume => Err(Error::InvalidArgument(format!(
            "{kind} 出力ファイルが既に存在します（resume 未指定）: {}",
            path.display()
        ))),
        Ok(m) => {
            let spb = SECTORS_PER_BLOCK as u64;
            let sectors = m.len() / sector_size as u64;
            let start = ((sectors / spb) * spb) as u32;
            Ok(Pending {
                path: path.to_path_buf(),
                start_sector: start,
            })
        }
        Err(_) => Ok(Pending {
            path: path.to_path_buf(),
            start_sector: 0,
        }),
    }
}

/// ジャーナルから `next=<sector>` を読む。
fn read_journal(path: &Path) -> Option<u32> {
    let mut s = String::new();
    File::open(path).ok()?.read_to_string(&mut s).ok()?;
    let line = s.lines().find(|l| l.starts_with("next="))?;
    line[5..].trim().parse().ok()
}

/// ジャーナルへ `next=<sector>` を書いて同期する。
fn write_journal(f: &mut File, next: u32) -> Result<()> {
    f.seek(SeekFrom::Start(0))?;
    f.set_len(0)?;
    f.write_all(format!("next={next}\n").as_bytes())?;
    f.sync_all()?;
    Ok(())
}
