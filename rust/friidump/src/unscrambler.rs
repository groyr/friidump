//! GameCube / Wii のスクランブル解除。
//!
//! C 実装 `libfriidump/unscrambler.c`（unscrambler 0.4, Victor Muñoz 由来）を移植する。
//! - グローバル `disctype` は `Unscrambler` 構造体へ移動（リファクタ項目 R1）
//! - seed キャッシュは C 版と同じ 80 スロット構成を忠実に再現する
//!   （位相ごとに最大 4 seed + 番兵スロット。C 版のスロット跨ぎ挙動も一致させる）
//!
//! ## seed キャッシュの構造（C 版準拠）
//! 16 位相それぞれに 4 スロットを割り当て、末尾の 1 スロットを番兵（`Terminator`）とする。
//! 位相 p の先頭は `p * MAX_SEEDS`。C 版は空きスロットが無い場合に次位相のスロットまで
//! 走査して書き込むため、同一のフラット配列で再現する。

use crate::constants::{
    BLOCK_SIZE, RAW_BLOCK_SIZE, RAW_SECTOR_SIZE, SECTORS_PER_BLOCK, SECTOR_SIZE,
};
use crate::ecma267::{edc_calc, Lfsr};
use crate::error::{Error, Result};
use crate::log::{self, Level};

/// 1 位相あたりの seed キャッシュ数。
const MAX_SEEDS: usize = 4;

/// 位相数（ブロック番号 mod 16）。
const PHASES: usize = 16;

/// フラットな seed スロット数（C 版 `(MAX_SEEDS + 1) * 16`）。
const SLOT_COUNT: usize = (MAX_SEEDS + 1) * PHASES;

/// EDC の計算対象長（末尾 4 バイトの EDC を除く）。
const EDC_LENGTH: usize = RAW_SECTOR_SIZE - 4;

/// seed 総当たりの上限（C 版 `j < 0x7FFF`）。
const SEED_BRUTEFORCE_LIMIT: u16 = 0x7FFF;

/// ディスク種別（スクランブル解除のレイアウト選択に使う）。
pub const DISCTYPE_NINTENDO: u8 = 0;
/// DVD 用レイアウト（C 版 `disctype == 3`）。
pub const DISCTYPE_DVD: u8 = 3;

/// seed キャッシュの 1 スロット。
enum Slot {
    /// 空きスロット（C 版 `seed == -1`）。
    Empty,
    /// 番兵スロット（C 版 `seed == -2`）。ここでキャッシュ探索は打ち切る。
    Terminator,
    /// 確定済み seed とそのストリーム暗号。
    Seed {
        seed: u16,
        cipher: Box<[u8; SECTOR_SIZE]>,
    },
}

/// スクランブル解除器。
pub struct Unscrambler {
    slots: Vec<Slot>,
    bruteforce_seeds: bool,
    disctype: u8,
}

impl Unscrambler {
    /// 新しいスクランブル解除器を生成する（C 版 `unscrambler_new`）。
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(SLOT_COUNT);
        for i in 0..SLOT_COUNT {
            // C 版の初期化を再現: 0..63 は空き、64 のみ番兵。
            if i == MAX_SEEDS * PHASES {
                slots.push(Slot::Terminator);
            } else {
                slots.push(Slot::Empty);
            }
        }
        Self {
            slots,
            bruteforce_seeds: true,
            disctype: DISCTYPE_NINTENDO,
        }
    }

    /// seed の総当たりを有効/無効にする（C 版 `unscrambler_set_bruteforce`）。
    pub fn set_bruteforce(&mut self, b: bool) {
        self.bruteforce_seeds = b;
        log::write(
            Level::Debug,
            None,
            format_args!(
                "Seed bruteforcing {}",
                if b { "enabled" } else { "disabled" }
            ),
        );
    }

    /// ディスク種別を設定する（C 版 `unscrambler_set_disctype`）。
    pub fn set_disctype(&mut self, disctype: u8) {
        self.disctype = disctype;
    }

    /// 現在のディスク種別を返す。
    pub fn disctype(&self) -> u8 {
        self.disctype
    }

    /// 指定スロットに seed を追加する（C 版 `add_seed`）。
    /// 番兵スロットだった場合は `None`（キャッシュ不足）。
    fn add_seed(&mut self, idx: usize, seed: u16) -> Option<usize> {
        match self.slots.get(idx) {
            Some(Slot::Terminator) | None => None,
            _ => {
                let mut lfsr = Lfsr::new(seed);
                let mut cipher = Box::new([0u8; SECTOR_SIZE]);
                for b in cipher.iter_mut() {
                    *b = lfsr.next_byte();
                }
                self.slots[idx] = Slot::Seed { seed, cipher };
                Some(idx)
            }
        }
    }

    /// 1 ブロック（16 セクタ）をスクランブル解除する（C 版 `unscrambler_unscramble_16sectors`）。
    ///
    /// `raw` は Nintendo レイアウトの場合に CPR_MAI 領域（2054..2060）が書き換わるため
    /// 可変参照で受け取る。
    pub fn unscramble_16sectors(
        &mut self,
        sector_no: u32,
        raw: &mut [u8; RAW_BLOCK_SIZE],
        out: &mut [u8; BLOCK_SIZE],
    ) -> Result<bool> {
        let base = (((sector_no as usize) / 16) & 0x0F) * MAX_SEEDS;

        // 1) キャッシュ探索。番兵または空きで打ち切る。
        let mut idx = base;
        let mut found: Option<usize> = None;
        while idx < SLOT_COUNT {
            match &self.slots[idx] {
                Slot::Seed { seed, .. } => {
                    if test_seed(raw, *seed) {
                        found = Some(idx);
                        break;
                    }
                    idx += 1;
                }
                _ => break,
            }
        }

        // 2) キャッシュに無ければ総当たりで探索し、見つかった seed をキャッシュする。
        if found.is_none() && self.bruteforce_seeds {
            let mut j: u16 = 0;
            while found.is_none() && j < SEED_BRUTEFORCE_LIMIT {
                if test_seed(raw, j) {
                    match self.add_seed(idx, j) {
                        Some(i) => found = Some(i),
                        None => {
                            return Err(Error::Unscramble(
                                "seed キャッシュの空きがありません".to_string(),
                            ))
                        }
                    }
                }
                j += 1;
            }
        }

        // 3) 復号
        match found {
            Some(i) => {
                let cipher: &[u8; SECTOR_SIZE] = match &self.slots[i] {
                    Slot::Seed { cipher, .. } => cipher,
                    _ => unreachable!("確定済みスロットは Seed のみ"),
                };
                let ok = unscramble_frame(self.disctype, cipher, raw, out);
                if ok {
                    Ok(true)
                } else {
                    Err(Error::Unscramble(format!(
                        "フレーム {sector_no} のスクランブル解除で EDC 不一致"
                    )))
                }
            }
            None => Err(Error::SeedNotFound(sector_no)),
        }
    }

    /// raw イメージファイルを ISO へ変換する（C 版 `unscrambler_unscramble_file`）。
    ///
    /// `progress` は `(start, sectors_done, total_sectors)` で呼ばれる。
    // C 版の `s % 320 == 0` をそのまま維持する（古い rustc でも動くよう is_multiple_of は使わない）
    #[allow(clippy::manual_is_multiple_of)]
    pub fn unscramble_file(
        &mut self,
        infile: &str,
        outfile: &str,
        mut progress: Option<&mut dyn FnMut(bool, u32, u32)>,
    ) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};

        let mut input = std::fs::File::open(infile).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("入力ファイル {infile}: {e}"),
            ))
        })?;
        let mut output = std::fs::File::create(outfile).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("出力ファイル {outfile}: {e}"),
            ))
        })?;

        let filesize = input.seek(SeekFrom::End(0))?;
        input.seek(SeekFrom::Start(0))?;
        let total_sectors = (filesize / RAW_SECTOR_SIZE as u64) as u32;

        if let Some(p) = progress.as_deref_mut() {
            p(true, 0, total_sectors);
        }

        let mut b_in = [0u8; RAW_BLOCK_SIZE];
        let mut b_out = [0u8; BLOCK_SIZE];
        let mut s: u32 = 0;

        loop {
            let r = read_full(&mut input, &mut b_in)?;
            if r == 0 {
                break;
            }
            if r < RAW_BLOCK_SIZE {
                crate::warning!("短いブロック読み出し ({r} バイト)、ゼロで埋めます");
                b_in[r..].fill(0);
            }

            self.unscramble_16sectors(s, &mut b_in, &mut b_out)
                .map_err(|e| Error::Unscramble(format!("セクタ {s} 付近: {e}")))?;
            output.write_all(&b_out)?;

            s += SECTORS_PER_BLOCK as u32;
            if let Some(p) = progress.as_deref_mut() {
                if s % 320 == 0 || s == total_sectors {
                    p(false, s, total_sectors);
                }
            }
        }

        output.flush()?;
        log::write(
            Level::Debug,
            None,
            format_args!("イメージのスクランブル解除に成功しました"),
        );
        Ok(())
    }
}

impl Default for Unscrambler {
    fn default() -> Self {
        Self::new()
    }
}

/// ブロックの先頭セクタの seed を EDC で検証する（C 版 `test_seed`）。
fn test_seed(raw: &[u8; RAW_BLOCK_SIZE], seed: u16) -> bool {
    let mut tmp = *raw;
    let mut lfsr = Lfsr::new(seed);
    for b in &mut tmp[12..EDC_LENGTH] {
        *b ^= lfsr.next_byte();
    }
    let calc = edc_calc(0, &tmp[..EDC_LENGTH]);
    let correct = u32::from_be_bytes([
        tmp[EDC_LENGTH],
        tmp[EDC_LENGTH + 1],
        tmp[EDC_LENGTH + 2],
        tmp[EDC_LENGTH + 3],
    ]);
    calc == correct
}

/// 1 ブロックを seed のストリーム暗号で復号する（C 版 `unscramble_frame`）。
///
/// Nintendo レイアウトでは `raw` の CPR_MAI 領域を書き換える（C 版と同じ副作用）。
fn unscramble_frame(
    disctype: u8,
    cipher: &[u8; SECTOR_SIZE],
    raw: &mut [u8; RAW_BLOCK_SIZE],
    out: &mut [u8; BLOCK_SIZE],
) -> bool {
    let mut ok = true;
    for j in 0..SECTORS_PER_BLOCK {
        let ro = j * RAW_SECTOR_SIZE;
        let oo = j * SECTOR_SIZE;

        let mut tmp = [0u8; RAW_SECTOR_SIZE];
        tmp.copy_from_slice(&raw[ro..ro + RAW_SECTOR_SIZE]);
        // データ部（raw[12:2060]）をストリーム暗号で XOR
        for (dst, c) in tmp[12..12 + SECTOR_SIZE].iter_mut().zip(cipher.iter()) {
            *dst ^= *c;
        }

        if disctype == DISCTYPE_DVD {
            out[oo..oo + SECTOR_SIZE].copy_from_slice(&tmp[12..12 + SECTOR_SIZE]);
        } else {
            out[oo..oo + SECTOR_SIZE].copy_from_slice(&tmp[6..6 + SECTOR_SIZE]);
            raw[ro + 2054..ro + 2060].copy_from_slice(&tmp[2054..2060]);
        }

        let calc = edc_calc(0, &tmp[..EDC_LENGTH]);
        let correct = u32::from_be_bytes([
            tmp[EDC_LENGTH],
            tmp[EDC_LENGTH + 1],
            tmp[EDC_LENGTH + 2],
            tmp[EDC_LENGTH + 3],
        ]);
        if calc != correct {
            log::write(
                Level::Debug,
                None,
                format_args!("EDC 不一致 (calc={calc:08x}, correct={correct:08x}, sector={j})"),
            );
            ok = false;
        }
    }
    ok
}

/// `read` が要求長を満たすか、EOF に達するまで読み切る。
fn read_full<R: std::io::Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = r.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用: ISO ブロックを指定 seed でスクランブルして正しい EDC 付き raw ブロックを作る。
    /// C 版 `unscramble_frame` の逆手順。
    fn scramble_block(
        iso: &[u8; BLOCK_SIZE],
        seed: u16,
        first_sector: u32,
    ) -> [u8; RAW_BLOCK_SIZE] {
        let mut lfsr = Lfsr::new(seed);
        let mut cipher = [0u8; SECTOR_SIZE];
        for b in cipher.iter_mut() {
            *b = lfsr.next_byte();
        }
        let mut raw = [0u8; RAW_BLOCK_SIZE];
        for j in 0..SECTORS_PER_BLOCK {
            let ro = j * RAW_SECTOR_SIZE;
            let oo = j * SECTOR_SIZE;
            let sn = first_sector + j as u32;
            // ヘッダ（先頭 6B）: セクタ番号を格納
            raw[ro] = 0;
            raw[ro + 1] = ((sn >> 16) & 0xFF) as u8;
            raw[ro + 2] = ((sn >> 8) & 0xFF) as u8;
            raw[ro + 3] = (sn & 0xFF) as u8;
            // ISO 先頭 6B は XOR 対象外
            raw[ro + 6..ro + 12].copy_from_slice(&iso[oo..oo + 6]);
            // ISO 残りはストリーム暗号で XOR
            for (dst, (s, c)) in raw[ro + 12..ro + 12 + SECTOR_SIZE - 6]
                .iter_mut()
                .zip(iso[oo + 6..oo + SECTOR_SIZE].iter().zip(cipher.iter()))
            {
                *dst = *s ^ *c;
            }
            // CPR_MAI（2054..2060）は任意値（ここでは 0）
            // EDC を計算して格納
            let mut tmp = [0u8; RAW_SECTOR_SIZE];
            tmp.copy_from_slice(&raw[ro..ro + RAW_SECTOR_SIZE]);
            let mut l = Lfsr::new(seed);
            for b in &mut tmp[12..EDC_LENGTH] {
                *b ^= l.next_byte();
            }
            let edc = edc_calc(0, &tmp[..EDC_LENGTH]);
            raw[ro + EDC_LENGTH..ro + RAW_SECTOR_SIZE].copy_from_slice(&edc.to_be_bytes());
        }
        raw
    }

    fn sample_iso() -> [u8; BLOCK_SIZE] {
        let mut iso = [0u8; BLOCK_SIZE];
        let mut x: u32 = 0x1234_5678;
        for b in iso.iter_mut() {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *b = (x >> 24) as u8;
        }
        iso
    }

    #[test]
    fn roundtrip_nintendo() {
        let iso = sample_iso();
        let seed = 0x2A7Bu16;
        let mut raw = scramble_block(&iso, seed, 0);
        let mut out = [0u8; BLOCK_SIZE];

        let mut u = Unscrambler::new();
        u.set_disctype(DISCTYPE_NINTENDO);
        u.set_bruteforce(true);
        assert!(u.unscramble_16sectors(0, &mut raw, &mut out).unwrap());
        assert_eq!(&out[..], &iso[..]);
    }

    #[test]
    fn roundtrip_dvd() {
        // DVD レイアウトは raw[12:2060] をそのまま出力に使うため、EDC 付き raw を用意する。
        let seed = 0x00FFu16;
        let mut raw = [0u8; RAW_BLOCK_SIZE];
        let mut expect = [0u8; BLOCK_SIZE];
        let mut lfsr = Lfsr::new(seed);
        let mut cipher = [0u8; SECTOR_SIZE];
        for b in cipher.iter_mut() {
            *b = lfsr.next_byte();
        }
        for j in 0..SECTORS_PER_BLOCK {
            let ro = j * RAW_SECTOR_SIZE;
            let oo = j * SECTOR_SIZE;
            for (i, (dst, c)) in raw[ro + 12..ro + 12 + SECTOR_SIZE]
                .iter_mut()
                .zip(cipher.iter())
                .enumerate()
            {
                let data = ((j * 7 + i) & 0xFF) as u8;
                *dst = data ^ *c;
                expect[oo + i] = data;
            }
            let mut tmp = [0u8; RAW_SECTOR_SIZE];
            tmp.copy_from_slice(&raw[ro..ro + RAW_SECTOR_SIZE]);
            let mut l = Lfsr::new(seed);
            for b in &mut tmp[12..EDC_LENGTH] {
                *b ^= l.next_byte();
            }
            let edc = edc_calc(0, &tmp[..EDC_LENGTH]);
            raw[ro + EDC_LENGTH..ro + RAW_SECTOR_SIZE].copy_from_slice(&edc.to_be_bytes());
        }

        let mut out = [0u8; BLOCK_SIZE];
        let mut u = Unscrambler::new();
        u.set_disctype(DISCTYPE_DVD);
        assert!(u.unscramble_16sectors(0, &mut raw, &mut out).unwrap());
        assert_eq!(&out[..], &expect[..]);
    }

    #[test]
    fn detects_corruption() {
        let iso = sample_iso();
        let mut raw = scramble_block(&iso, 0x0102, 0);
        // データを破壊すると seed が見つからず失敗するはず
        raw[100] ^= 0xFF;
        let mut out = [0u8; BLOCK_SIZE];
        let mut u = Unscrambler::new();
        assert!(u.unscramble_16sectors(0, &mut raw, &mut out).is_err());
    }

    #[test]
    fn caches_seed_and_reuses() {
        let iso = sample_iso();
        let seed = 0x0033u16;
        let mut u = Unscrambler::new();
        // 1 回目（総当たりで発見）と 2 回目（キャッシュ利用）で同一結果
        for _ in 0..2 {
            let mut raw = scramble_block(&iso, seed, 0);
            let mut out = [0u8; BLOCK_SIZE];
            assert!(u.unscramble_16sectors(0, &mut raw, &mut out).unwrap());
            assert_eq!(&out[..], &iso[..]);
        }
    }
}
