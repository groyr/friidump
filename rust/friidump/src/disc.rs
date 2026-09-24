//! ディスク読み出しの中心（method11/12 の fast 方式）。
//!
//! C 実装 `libfriidump/disc_fast.c` と `libfriidump/disc.c` のキャッシュ/方式ディスパッチを
//! 移植する。デバイスは `ScsiDevice` 抽象越しに扱うため、テストではモックを差し込める。
//!
//! ## fast 方式の原理（method11/12）
//! 通常 READ(12) が返すホストデータ `rd` は
//! `rd = ISO[6:2048] XOR gc_cipher(phase) XOR drive_cipher(phase)` であり、
//! GC 側・ドライブ側の cipher はともに 16 ブロック周期。代表 16 ブロックを E7 で
//! 生フレームごと読み、`corr[phase] = drive_cipher XOR gc_cipher` を校正しておけば
//! `out = rd XOR corr` で ISO を復元できる。各セクタ先頭 6B は E7 から補う。

use crate::cache::{BlockCache, DEFAULT_CACHE_SIZE};
use crate::constants::{
    BLOCK_SIZE, RAW_BLOCK_SIZE, RAW_SECTOR_SIZE, SECTORS_PER_BLOCK, SECTOR_SIZE,
};
use crate::drive::device::ScsiDevice;
use crate::drive::dvd::DvdDrive;
use crate::error::{Error, Result};
use crate::metadata::DiscType;
use crate::read::{compute_params, ReadMethod, ReadParams};
use crate::unscrambler::Unscrambler;

/// 1 ブロックの生フレーム先頭（E7 で取得する 12 バイト）。
const HEAD_LEN: usize = 12;
/// 生フレーム末尾（E7 で取得する 10 バイト: CPR_MAI 6B + EDC 4B）。
const TAIL_LEN: usize = 10;
/// 生フレーム末尾の開始オフセット。
const TAIL_OFFSET: usize = RAW_SECTOR_SIZE - TAIL_LEN; // 2054
/// 第 1 層のセクタ番号オフセット。
const SN_OFFSET_LAYER1: u32 = 0x30000;

/// ディスク読み出し器。
pub struct Disc<D: ScsiDevice> {
    /// ドライブ。
    pub drive: DvdDrive<D>,
    /// スクランブル解除器。
    pub unscrambler: Unscrambler,
    /// ブロックキャッシュ。
    pub cache: BlockCache,
    /// ディスク種別。
    pub disc_type: DiscType,
    /// 総セクタ数。
    pub sectors_no: u32,
    /// layer break。
    pub layerbreak: u32,
    /// 読み出し方式。
    pub read_method: ReadMethod,
    /// 読み出しパラメータ。
    pub params: ReadParams,
    /// スクランブル解除を行うか。
    pub unscrambling: bool,

    // ---- fast 方式 (method11) の補正テーブル ----
    fast_ready: bool,
    fast_corr: Box<[[u8; SECTOR_SIZE - 6]; SECTORS_PER_BLOCK]>,
    fast_ready2: bool,
    fast_corr2: Box<[[u8; SECTOR_SIZE - 6]; SECTORS_PER_BLOCK]>,
    // ---- method12 のドライブ cipher ----
    fast12_ready: bool,
    drive_cipher12: Box<[[u8; SECTOR_SIZE]; SECTORS_PER_BLOCK]>,
    fast12_ready2: bool,
    drive_cipher12_2: Box<[[u8; SECTOR_SIZE]; SECTORS_PER_BLOCK]>,
    // ---- 二層対応 ----
    layer_sn_offset2: u32,
    layer2_ready: bool,
}

impl<D: ScsiDevice> Disc<D> {
    /// ドライブからディスク読み出し器を生成する。
    pub fn new(drive: DvdDrive<D>) -> Result<Self> {
        let mut unscrambler = Unscrambler::new();
        unscrambler.set_bruteforce(true);
        Ok(Self {
            drive,
            unscrambler,
            cache: BlockCache::new(DEFAULT_CACHE_SIZE)?,
            disc_type: DiscType::GameCube,
            sectors_no: 0,
            layerbreak: 0,
            read_method: ReadMethod::M12,
            params: compute_params(ReadMethod::M12, None, None)?,
            unscrambling: true,
            fast_ready: false,
            fast_corr: Box::new([[0u8; SECTOR_SIZE - 6]; SECTORS_PER_BLOCK]),
            fast_ready2: false,
            fast_corr2: Box::new([[0u8; SECTOR_SIZE - 6]; SECTORS_PER_BLOCK]),
            fast12_ready: false,
            drive_cipher12: Box::new([[0u8; SECTOR_SIZE]; SECTORS_PER_BLOCK]),
            fast12_ready2: false,
            drive_cipher12_2: Box::new([[0u8; SECTOR_SIZE]; SECTORS_PER_BLOCK]),
            layer_sn_offset2: 0,
            layer2_ready: false,
        })
    }

    /// 読み出し方式を設定する。
    pub fn set_read_method(&mut self, method: ReadMethod) -> Result<()> {
        self.read_method = method;
        self.params = compute_params(method, None, None)?;
        Ok(())
    }

    /// スクランブル解除の有効/無効を設定する。
    pub fn set_unscrambling(&mut self, on: bool) {
        self.unscrambling = on;
        self.unscrambler
            .set_disctype(if self.disc_type == DiscType::Dvd {
                3
            } else {
                0
            });
    }

    /// セクタをキャッシュへ読み込む（C 版 `disc_read_sector`）。データは `sector` で取得する。
    pub fn read_sector(&mut self, sector_no: u32) -> Result<()> {
        let block = sector_no / SECTORS_PER_BLOCK as u32;
        if self.cache.lookup(block).is_some() {
            return Ok(());
        }
        match self.read_method {
            ReadMethod::M11 => self.read_block_11(block),
            ReadMethod::M12 => self.read_block_12(block),
            other => Err(Error::Unsupported(format!(
                "method{} の読み出しは未実装です",
                other.id()
            ))),
        }
    }

    /// 読み込み済みセクタの `(data, raw)` を返す（C 版 `disc_read_sector` の出力に相当）。
    ///
    /// `data` はスクランブル解除済み 2048 バイト、`raw` は 2064 バイト。
    pub fn sector(&self, sector_no: u32) -> Option<(&[u8; BLOCK_SIZE], &[u8; RAW_BLOCK_SIZE])> {
        let block = sector_no / SECTORS_PER_BLOCK as u32;
        self.cache.lookup(block)
    }

    /// 指定 LBA が第 2 層かどうか。
    fn is_layer2(&self, lba: u32) -> bool {
        self.layerbreak > 0 && self.disc_type == DiscType::WiiDl && lba >= self.layerbreak
    }

    /// 層の先頭ブロックの LBA（層境界を 16 セクタ境界に丸めたもの）。
    fn layer_start_lba(&self) -> u32 {
        (self.layerbreak / SECTORS_PER_BLOCK as u32) * SECTORS_PER_BLOCK as u32
    }

    /// 生フレームのセクタ番号オフセット（C 版 `disc_fast_sn_offset`）。
    fn sn_offset(&self, blk: u32) -> u32 {
        let lba = blk * SECTORS_PER_BLOCK as u32;
        if self.is_layer2(lba) {
            self.layer_sn_offset2
        } else {
            SN_OFFSET_LAYER1
        }
    }

    /// 1 ブロックの生フレームとホストデータを取得する（C 版 `disc_fast_fetch_raw`）。
    fn fetch_raw(
        &mut self,
        blk: u32,
        rd: &mut [u8; BLOCK_SIZE],
        rawbuf: &mut [u8; RAW_BLOCK_SIZE],
    ) -> Result<()> {
        let lba = blk * SECTORS_PER_BLOCK as u32;
        for _ in 0..5 {
            if self.drive.read_sector_streaming_discard(lba).is_err() {
                continue;
            }
            if self.drive.read_sector_streaming(lba, rd).is_err() {
                continue;
            }
            if self
                .drive
                .memdump(0, 1, RAW_BLOCK_SIZE as u32, rawbuf)
                .is_err()
            {
                continue;
            }
            return Ok(());
        }
        Err(Error::Other(format!("生フレーム取得に失敗: block {blk}")))
    }

    /// 1 ブロックを raw 経路で読み、ホストデータ `rd` と復号済み `p` を返す
    /// （C 版 `disc_fast_read_raw_block`）。
    fn read_raw_block(
        &mut self,
        blk: u32,
        rd: &mut [u8; BLOCK_SIZE],
        rawout: Option<&mut [u8; RAW_BLOCK_SIZE]>,
        p: &mut [u8; BLOCK_SIZE],
    ) -> Result<()> {
        let lba = blk * SECTORS_PER_BLOCK as u32;
        let mut rawbuf = [0u8; RAW_BLOCK_SIZE];
        self.fetch_raw(blk, rd, &mut rawbuf)?;
        if let Some(out) = rawout {
            out.copy_from_slice(&rawbuf);
        }
        if self
            .unscrambler
            .unscramble_16sectors(lba, &mut rawbuf, p)
            .is_err()
        {
            return Err(Error::Other(format!(
                "raw ブロックのスクランブル解除に失敗: block {blk}"
            )));
        }
        Ok(())
    }

    /// method11 の補正テーブルを作る（C 版 `disc_fast_calibrate`）。
    fn calibrate11(&mut self) -> Result<()> {
        if self.fast_ready {
            return Ok(());
        }
        for m in 0..SECTORS_PER_BLOCK {
            let mut rd = [0u8; BLOCK_SIZE];
            let mut p = [0u8; BLOCK_SIZE];
            if self
                .read_raw_block(16 + m as u32, &mut rd, None, &mut p)
                .is_err()
            {
                return Err(Error::Other(format!(
                    "method11 校正に失敗: block {}",
                    16 + m
                )));
            }
            for k in 0..SECTORS_PER_BLOCK {
                for i in 0..SECTOR_SIZE - 6 {
                    self.fast_corr[m][i] = rd[k * SECTOR_SIZE + i] ^ p[k * SECTOR_SIZE + 6 + i];
                }
            }
        }
        self.fast_ready = true;
        Ok(())
    }

    /// 第 2 層用 method11 補正テーブルを作る（C 版 `disc_fast_calibrate_layer2`）。
    fn calibrate11_layer2(&mut self) -> Result<()> {
        if self.fast_ready2 {
            return Ok(());
        }
        self.detect_layer2()?;
        let base_blk = self.layer_start_lba() / SECTORS_PER_BLOCK as u32 + 16;
        for m in 0..SECTORS_PER_BLOCK {
            let mut rd = [0u8; BLOCK_SIZE];
            let mut p = [0u8; BLOCK_SIZE];
            if self
                .read_raw_block(base_blk + m as u32, &mut rd, None, &mut p)
                .is_err()
            {
                return Err(Error::Other(format!(
                    "method11 第2層校正に失敗: block {}",
                    base_blk + m as u32
                )));
            }
            for k in 0..SECTORS_PER_BLOCK {
                for i in 0..SECTOR_SIZE - 6 {
                    self.fast_corr2[m][i] = rd[k * SECTOR_SIZE + i] ^ p[k * SECTOR_SIZE + 6 + i];
                }
            }
        }
        self.fast_ready2 = true;
        Ok(())
    }

    /// 第 2 層の物理セクタ番号オフセットを検出する（C 版 `disc_fast_detect_layer2`）。
    fn detect_layer2(&mut self) -> Result<()> {
        if self.layer2_ready {
            return Ok(());
        }
        let start = self.layer_start_lba();
        if self.layerbreak == 0 || start == 0 {
            return Err(Error::Other(
                "単層ディスクでは第2層検出できません".to_string(),
            ));
        }
        let mut rd = [0u8; BLOCK_SIZE];
        let mut rawbuf = [0u8; RAW_BLOCK_SIZE];
        self.fetch_raw(start / SECTORS_PER_BLOCK as u32, &mut rd, &mut rawbuf)?;
        let sn = ((rawbuf[1] as u32) << 16) | ((rawbuf[2] as u32) << 8) | rawbuf[3] as u32;
        self.layer_sn_offset2 = sn - start;
        self.layer2_ready = true;
        Ok(())
    }

    /// method11 で 1 ブロックを読む（C 版 `disc_read_sector_11`）。
    fn read_block_11(&mut self, blk: u32) -> Result<()> {
        let lba = blk * SECTORS_PER_BLOCK as u32;
        let layer2 = self.is_layer2(lba);
        let is_dvd = self.disc_type == DiscType::Dvd;
        let mut out = [0u8; BLOCK_SIZE];

        // block0 と第2層先頭ブロックは seed が例外なので raw 経路
        if blk == 0 || (layer2 && lba == self.layer_start_lba()) {
            let mut rd = [0u8; BLOCK_SIZE];
            self.read_raw_block(blk, &mut rd, None, &mut out)?;
            self.cache.add_block(blk, &out, is_dvd);
            return Ok(());
        }

        if layer2 {
            self.calibrate11_layer2()?;
        } else {
            self.calibrate11()?;
        }
        let m = (blk % SECTORS_PER_BLOCK as u32) as usize;
        let corr = if layer2 {
            self.fast_corr2[m]
        } else {
            self.fast_corr[m]
        };
        let sn_off = self.sn_offset(blk);

        for _retry in 0..5 {
            let mut rd = [0u8; BLOCK_SIZE];
            if self.drive.read_sector_streaming_discard(lba).is_err() {
                continue;
            }
            if self.drive.read_sector_streaming(lba, &mut rd).is_err() {
                continue;
            }
            let mut ok = true;
            for k in 0..SECTORS_PER_BLOCK {
                let mut e = [0u8; HEAD_LEN];
                if self
                    .drive
                    .memdump((k * RAW_SECTOR_SIZE) as u32, 1, HEAD_LEN as u32, &mut e)
                    .is_err()
                {
                    ok = false;
                    break;
                }
                let sn = ((e[1] as u32) << 16) | ((e[2] as u32) << 8) | e[3] as u32;
                if sn != lba + k as u32 + sn_off {
                    ok = false;
                    break;
                }
                out[k * SECTOR_SIZE..k * SECTOR_SIZE + 6].copy_from_slice(&e[6..12]);
                for i in 0..SECTOR_SIZE - 6 {
                    out[k * SECTOR_SIZE + 6 + i] = rd[k * SECTOR_SIZE + i] ^ corr[i];
                }
            }
            if ok {
                self.cache.add_block(blk, &out, is_dvd);
                return Ok(());
            }
        }

        // フォールバック: raw 経路
        let mut rd = [0u8; BLOCK_SIZE];
        self.read_raw_block(blk, &mut rd, None, &mut out)?;
        self.cache.add_block(blk, &out, is_dvd);
        Ok(())
    }

    /// method12 のドライブ cipher を作る（C 版 `disc_fast_calibrate12`）。
    fn calibrate12(&mut self) -> Result<()> {
        if self.fast12_ready {
            return Ok(());
        }
        for m in 0..SECTORS_PER_BLOCK {
            let mut rd = [0u8; BLOCK_SIZE];
            let mut rawb = [0u8; RAW_BLOCK_SIZE];
            let mut p = [0u8; BLOCK_SIZE];
            if self
                .read_raw_block(16 + m as u32, &mut rd, Some(&mut rawb), &mut p)
                .is_err()
            {
                return Err(Error::Other(format!(
                    "method12 校正に失敗: block {}",
                    16 + m
                )));
            }
            for k in 0..SECTORS_PER_BLOCK {
                for i in 0..SECTOR_SIZE {
                    self.drive_cipher12[m][i] =
                        rd[k * SECTOR_SIZE + i] ^ rawb[k * RAW_SECTOR_SIZE + 12 + i];
                }
            }
        }
        self.fast12_ready = true;
        Ok(())
    }

    /// 第 2 層用 method12 cipher を作る（C 版 `disc_fast_calibrate12_layer2`）。
    fn calibrate12_layer2(&mut self) -> Result<()> {
        if self.fast12_ready2 {
            return Ok(());
        }
        self.detect_layer2()?;
        let base_blk = self.layer_start_lba() / SECTORS_PER_BLOCK as u32 + 16;
        for m in 0..SECTORS_PER_BLOCK {
            let mut rd = [0u8; BLOCK_SIZE];
            let mut rawb = [0u8; RAW_BLOCK_SIZE];
            let mut p = [0u8; BLOCK_SIZE];
            if self
                .read_raw_block(base_blk + m as u32, &mut rd, Some(&mut rawb), &mut p)
                .is_err()
            {
                return Err(Error::Other(format!(
                    "method12 第2層校正に失敗: block {}",
                    base_blk + m as u32
                )));
            }
            for k in 0..SECTORS_PER_BLOCK {
                for i in 0..SECTOR_SIZE {
                    self.drive_cipher12_2[m][i] =
                        rd[k * SECTOR_SIZE + i] ^ rawb[k * RAW_SECTOR_SIZE + 12 + i];
                }
            }
        }
        self.fast12_ready2 = true;
        Ok(())
    }

    /// method12 で 1 ブロックを読む（C 版 `disc_read_sector_12`）。
    fn read_block_12(&mut self, blk: u32) -> Result<()> {
        let lba = blk * SECTORS_PER_BLOCK as u32;
        let layer2 = self.is_layer2(lba);
        let mut out = [0u8; BLOCK_SIZE];

        if blk == 0 || (layer2 && lba == self.layer_start_lba()) {
            let mut rd = [0u8; BLOCK_SIZE];
            let mut rawtrue = [0u8; RAW_BLOCK_SIZE];
            self.read_raw_block(blk, &mut rd, Some(&mut rawtrue), &mut out)?;
            self.cache.add_block_raw(blk, &out, &rawtrue);
            return Ok(());
        }

        if layer2 {
            self.calibrate12_layer2()?;
        } else {
            self.calibrate12()?;
        }
        let m = (blk % SECTORS_PER_BLOCK as u32) as usize;
        let cipher = if layer2 {
            self.drive_cipher12_2[m]
        } else {
            self.drive_cipher12[m]
        };
        let sn_off = self.sn_offset(blk);

        for _retry in 0..5 {
            let mut rd = [0u8; BLOCK_SIZE];
            if self.drive.read_sector_streaming_discard(lba).is_err() {
                continue;
            }
            if self.drive.read_sector_streaming(lba, &mut rd).is_err() {
                continue;
            }
            let mut rawb = [0u8; RAW_BLOCK_SIZE];
            let mut ok = true;
            for k in 0..SECTORS_PER_BLOCK {
                let mut head = [0u8; HEAD_LEN];
                if self
                    .drive
                    .memdump((k * RAW_SECTOR_SIZE) as u32, 1, HEAD_LEN as u32, &mut head)
                    .is_err()
                {
                    ok = false;
                    break;
                }
                let sn = ((head[1] as u32) << 16) | ((head[2] as u32) << 8) | head[3] as u32;
                if sn != lba + k as u32 + sn_off {
                    ok = false;
                    break;
                }
                let mut tail = [0u8; TAIL_LEN];
                if self
                    .drive
                    .memdump(
                        (k * RAW_SECTOR_SIZE + TAIL_OFFSET) as u32,
                        1,
                        TAIL_LEN as u32,
                        &mut tail,
                    )
                    .is_err()
                {
                    ok = false;
                    break;
                }
                rawb[k * RAW_SECTOR_SIZE..k * RAW_SECTOR_SIZE + HEAD_LEN].copy_from_slice(&head);
                for i in 0..SECTOR_SIZE {
                    rawb[k * RAW_SECTOR_SIZE + 12 + i] = rd[k * SECTOR_SIZE + i] ^ cipher[i];
                }
                rawb[k * RAW_SECTOR_SIZE + TAIL_OFFSET..(k + 1) * RAW_SECTOR_SIZE]
                    .copy_from_slice(&tail);
            }
            if !ok {
                continue;
            }

            // EDC 検証（unscramble が rawb を書き換えるため生フレームを退避）
            let mut rawtrue = [0u8; RAW_BLOCK_SIZE];
            rawtrue.copy_from_slice(&rawb);
            if self
                .unscrambler
                .unscramble_16sectors(lba, &mut rawb, &mut out)
                .is_err()
            {
                continue;
            }
            self.cache.add_block_raw(blk, &out, &rawtrue);
            return Ok(());
        }

        // フォールバック: raw 経路
        let mut rd = [0u8; BLOCK_SIZE];
        let mut rawtrue = [0u8; RAW_BLOCK_SIZE];
        self.read_raw_block(blk, &mut rd, Some(&mut rawtrue), &mut out)?;
        self.cache.add_block_raw(blk, &out, &rawtrue);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::device::{ScsiDevice, ScsiOutcome};
    use crate::drive::mmc::Command;
    use crate::drive::profile::{MemdumpKind, ReadFamily};
    use crate::ecma267::{edc_calc, Lfsr};

    /// ISO 内容を決定的に生成する。
    fn iso_byte(block: u32, j: usize) -> u8 {
        ((block as usize).wrapping_mul(131).wrapping_add(j) & 0xFF) as u8
    }

    /// GameCube ドライブを模擬する仮想デバイス。
    ///
    /// READ(12) は `raw[12:2060] XOR drive_cipher` を返し、E7 は直前に READ した
    /// ブロックの生フレームを返す。生フレームは gc_seed で正しい EDC 付きに組み立てる。
    struct VirtualGcDrive {
        gc_seed: [u16; SECTORS_PER_BLOCK],
        drive_cipher: Vec<[u8; SECTOR_SIZE]>,
        current: Option<u32>,
    }

    impl VirtualGcDrive {
        fn new() -> Self {
            let mut gc_seed = [0u16; SECTORS_PER_BLOCK];
            let mut drive_cipher = Vec::with_capacity(SECTORS_PER_BLOCK);
            for (m, seed) in gc_seed.iter_mut().enumerate() {
                *seed = (m as u16) + 1; // 1..16（総当たりが速い）
                let mut lfsr = Lfsr::new(0x3000 + m as u16);
                let mut c = [0u8; SECTOR_SIZE];
                for b in c.iter_mut() {
                    *b = lfsr.next_byte();
                }
                drive_cipher.push(c);
            }
            Self {
                gc_seed,
                drive_cipher,
                current: None,
            }
        }

        fn gc_cipher(&self, phase: usize) -> [u8; SECTOR_SIZE] {
            let mut lfsr = Lfsr::new(self.gc_seed[phase]);
            let mut c = [0u8; SECTOR_SIZE];
            for b in c.iter_mut() {
                *b = lfsr.next_byte();
            }
            c
        }

        /// ブロックの生フレームを組み立てる。
        fn raw_block(&self, block: u32) -> [u8; RAW_BLOCK_SIZE] {
            let phase = (block % SECTORS_PER_BLOCK as u32) as usize;
            let gc = self.gc_cipher(phase);
            let lba = block * SECTORS_PER_BLOCK as u32;
            let mut raw = [0u8; RAW_BLOCK_SIZE];
            for k in 0..SECTORS_PER_BLOCK {
                let ro = k * RAW_SECTOR_SIZE;
                let oo = k * SECTOR_SIZE;
                let sn = lba + k as u32;
                raw[ro] = 0;
                raw[ro + 1] = ((sn >> 16) & 0xFF) as u8;
                raw[ro + 2] = ((sn >> 8) & 0xFF) as u8;
                raw[ro + 3] = (sn & 0xFF) as u8;
                for i in 0..6 {
                    raw[ro + 6 + i] = iso_byte(block, oo + i);
                }
                for i in 0..SECTOR_SIZE - 6 {
                    raw[ro + 12 + i] = iso_byte(block, oo + 6 + i) ^ gc[i];
                }
                // EDC を正しく計算（unscrambler が seed を特定できるように）
                let mut tmp = [0u8; RAW_SECTOR_SIZE];
                tmp.copy_from_slice(&raw[ro..ro + RAW_SECTOR_SIZE]);
                let mut l = Lfsr::new(self.gc_seed[phase]);
                for b in &mut tmp[12..RAW_SECTOR_SIZE - 4] {
                    *b ^= l.next_byte();
                }
                let edc = edc_calc(0, &tmp[..RAW_SECTOR_SIZE - 4]);
                raw[ro + RAW_SECTOR_SIZE - 4..ro + RAW_SECTOR_SIZE]
                    .copy_from_slice(&edc.to_be_bytes());
            }
            raw
        }
    }

    impl ScsiDevice for VirtualGcDrive {
        fn execute(&mut self, command: &Command, data: &mut [u8]) -> Result<ScsiOutcome> {
            match command.cdb[0] {
                0xA8 => {
                    let lba = u32::from_be_bytes([
                        command.cdb[2],
                        command.cdb[3],
                        command.cdb[4],
                        command.cdb[5],
                    ]);
                    let block = lba / SECTORS_PER_BLOCK as u32;
                    self.current = Some(block);
                    if !data.is_empty() {
                        let raw = self.raw_block(block);
                        let phase = (block % SECTORS_PER_BLOCK as u32) as usize;
                        for k in 0..SECTORS_PER_BLOCK {
                            for i in 0..SECTOR_SIZE {
                                data[k * SECTOR_SIZE + i] =
                                    raw[k * RAW_SECTOR_SIZE + 12 + i] ^ self.drive_cipher[phase][i];
                            }
                        }
                    }
                    Ok(ScsiOutcome::default())
                }
                0xE7 => {
                    let abs = u32::from_be_bytes([
                        command.cdb[6],
                        command.cdb[7],
                        command.cdb[8],
                        command.cdb[9],
                    ]);
                    let len = ((command.cdb[10] as usize) << 8) | command.cdb[11] as usize;
                    let block = self
                        .current
                        .ok_or_else(|| Error::Other("current block 未設定".to_string()))?;
                    let raw = self.raw_block(block);
                    let rel = (abs - crate::drive::dvd::MN103S_MEM_BASE) as usize;
                    let n = len.min(data.len());
                    data[..n].copy_from_slice(&raw[rel..rel + n]);
                    Ok(ScsiOutcome::default())
                }
                _ => Ok(ScsiOutcome::default()),
            }
        }
    }

    fn make_disc(method: ReadMethod) -> Disc<VirtualGcDrive> {
        let mut drive = DvdDrive::new(VirtualGcDrive::new(), -1);
        drive.memdump_kind = MemdumpKind::HitachiMn103s;
        drive.family = ReadFamily::HitachiType1;
        drive.def_method = method.id();
        let mut disc = Disc::new(drive).unwrap();
        disc.disc_type = DiscType::GameCube;
        disc.set_unscrambling(true);
        disc.set_read_method(method).unwrap();
        disc
    }

    fn expected_block(block: u32) -> [u8; BLOCK_SIZE] {
        let mut b = [0u8; BLOCK_SIZE];
        for (j, v) in b.iter_mut().enumerate() {
            *v = iso_byte(block, j);
        }
        b
    }

    #[test]
    fn method11_recovers_iso() {
        let mut disc = make_disc(ReadMethod::M11);
        // 校正(16..31) とデータブロック(20) を含む範囲を読む
        for sector in [0u32, 16 * 20, 16 * 21] {
            disc.read_sector(sector).unwrap();
        }
        let (data, _raw) = disc.sector(16 * 20).unwrap();
        assert_eq!(data, &expected_block(20));
        let (data21, _) = disc.sector(16 * 21).unwrap();
        assert_eq!(data21, &expected_block(21));
    }

    #[test]
    fn method12_recovers_iso_and_true_raw() {
        let mut disc = make_disc(ReadMethod::M12);
        for sector in [0u32, 16 * 20, 16 * 21] {
            disc.read_sector(sector).unwrap();
        }
        let (data, raw) = disc.sector(16 * 20).unwrap();
        assert_eq!(data, &expected_block(20));
        // raw は真スクランブル像: raw[12:2060] XOR gc_cipher == ISO[6:2048]
        // ここでは raw[6:12] == ISO[0:6] と raw[12..] の整合を簡易確認
        assert_eq!(&raw[6..12], &data[..6]);
    }

    #[test]
    fn method11_offline_calibration_only() {
        // block0 は raw 経路（seed 例外）
        let mut disc = make_disc(ReadMethod::M11);
        disc.read_sector(0).unwrap();
        let (data, _) = disc.sector(0).unwrap();
        assert_eq!(data, &expected_block(0));
    }
}
