//! 16 セクタブロック単位の読み出しキャッシュ。
//!
//! 元とした C 実装の `disc_cache_*` を移植する。
//! raw ポインタ配列と `INVALID` センチネルをやめ、`Option<u32>` で安全に保持する
//! （リファクタ項目 R1/R3）。リング状に `size` ブロックを保持する。

use crate::constants::{
    BLOCK_SIZE, RAW_BLOCK_SIZE, RAW_SECTOR_SIZE, SECTORS_PER_BLOCK, SECTOR_SIZE,
};
use crate::error::{Error, Result};

/// 既定のキャッシュブロック数（C 版 `DISC_DEFAULT_CACHE_SIZE`）。
pub const DEFAULT_CACHE_SIZE: usize = 40;

/// 最小キャッシュブロック数（C 版 `DISC_MINIMUM_CACHE_SIZE`）。
pub const MINIMUM_CACHE_SIZE: usize = 5;

/// 1 ブロック分のキャッシュエントリ。
struct Entry {
    /// このスロットが保持するブロック番号。`None` は未使用。
    map: Option<u32>,
    /// スクランブル解除済みデータ（BLOCK_SIZE）。
    data: Box<[u8; BLOCK_SIZE]>,
    /// 生フレーム（RAW_BLOCK_SIZE）。
    raw: Box<[u8; RAW_BLOCK_SIZE]>,
}

/// ブロックキャッシュ。
pub struct BlockCache {
    entries: Vec<Entry>,
}

impl BlockCache {
    /// 指定ブロック数のキャッシュを生成する。
    pub fn new(size: usize) -> Result<Self> {
        if size < MINIMUM_CACHE_SIZE {
            return Err(Error::InvalidArgument(format!(
                "キャッシュサイズが不正です: {size}（{MINIMUM_CACHE_SIZE} 以上が必要）"
            )));
        }
        let mut entries = Vec::with_capacity(size);
        for _ in 0..size {
            entries.push(Entry {
                map: None,
                data: Box::new([0u8; BLOCK_SIZE]),
                raw: Box::new([0u8; RAW_BLOCK_SIZE]),
            });
        }
        Ok(Self { entries })
    }

    /// 保持ブロック数を返す。
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// ブロック番号からスロット位置を求める（C 版 `block % cache_size`）。
    fn pos(&self, block: u32) -> usize {
        (block as usize) % self.entries.len()
    }

    /// ブロックがキャッシュにあれば `(data, raw)` を返す（C 版 `disc_cache_lookup_block`）。
    pub fn lookup(&self, block: u32) -> Option<(&[u8; BLOCK_SIZE], &[u8; RAW_BLOCK_SIZE])> {
        let e = &self.entries[self.pos(block)];
        if e.map == Some(block) {
            Some((&e.data, &e.raw))
        } else {
            None
        }
    }

    /// スクランブル解除済みデータを格納し、raw フレームを再構成する（C 版 `disc_cache_add_block`）。
    ///
    /// `is_dvd` が真なら raw のデータ部はオフセット 12、偽（GC/Wii）ならオフセット 6 に置く。
    pub fn add_block(&mut self, block: u32, data: &[u8; BLOCK_SIZE], is_dvd: bool) {
        let pos = self.pos(block);
        let e = &mut self.entries[pos];
        e.data.copy_from_slice(data);
        let offset = if is_dvd { 12 } else { 6 };
        for cnt in 0..SECTORS_PER_BLOCK {
            let dst = cnt * RAW_SECTOR_SIZE + offset;
            let src = cnt * SECTOR_SIZE;
            e.raw[dst..dst + SECTOR_SIZE].copy_from_slice(&data[src..src + SECTOR_SIZE]);
        }
        e.map = Some(block);
    }

    /// raw をそのまま（真スクランブル像のまま）格納する（C 版 `disc_cache_add_block_raw`）。
    ///
    /// method12 のように DIC/redump 互換の生 raw を保存したい場合に使う。
    pub fn add_block_raw(
        &mut self,
        block: u32,
        data: &[u8; BLOCK_SIZE],
        rawtrue: &[u8; RAW_BLOCK_SIZE],
    ) {
        let pos = self.pos(block);
        let e = &mut self.entries[pos];
        e.data.copy_from_slice(data);
        e.raw.copy_from_slice(rawtrue);
        e.map = Some(block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_data(seed: u8) -> [u8; BLOCK_SIZE] {
        let mut d = [0u8; BLOCK_SIZE];
        for (i, b) in d.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        d
    }

    #[test]
    fn rejects_too_small() {
        assert!(BlockCache::new(4).is_err());
        assert!(BlockCache::new(MINIMUM_CACHE_SIZE).is_ok());
    }

    #[test]
    fn add_lookup_and_miss() {
        let mut c = BlockCache::new(8).unwrap();
        assert!(c.lookup(3).is_none());
        c.add_block(3, &block_data(1), false);
        let (d, _r) = c.lookup(3).unwrap();
        assert_eq!(d, &block_data(1));
        assert!(c.lookup(4).is_none());
    }

    #[test]
    fn gc_raw_layout_offsets() {
        let mut c = BlockCache::new(8).unwrap();
        let data = block_data(7);
        c.add_block(0, &data, false);
        let (_d, raw) = c.lookup(0).unwrap();
        // GC は offset 6 にデータを置く
        assert_eq!(&raw[6..6 + SECTOR_SIZE], &data[..SECTOR_SIZE]);
        // 2 セクタ目
        assert_eq!(
            &raw[RAW_SECTOR_SIZE + 6..RAW_SECTOR_SIZE + 6 + SECTOR_SIZE],
            &data[SECTOR_SIZE..2 * SECTOR_SIZE]
        );
    }

    #[test]
    fn dvd_raw_layout_offsets() {
        let mut c = BlockCache::new(8).unwrap();
        let data = block_data(9);
        c.add_block(1, &data, true);
        let (_d, raw) = c.lookup(1).unwrap();
        assert_eq!(&raw[12..12 + SECTOR_SIZE], &data[..SECTOR_SIZE]);
    }

    #[test]
    fn raw_true_is_preserved() {
        let mut c = BlockCache::new(8).unwrap();
        let data = block_data(2);
        let mut rawtrue = [0u8; RAW_BLOCK_SIZE];
        for (i, b) in rawtrue.iter_mut().enumerate() {
            *b = (i & 0xFF) as u8;
        }
        c.add_block_raw(5, &data, &rawtrue);
        let (d, r) = c.lookup(5).unwrap();
        assert_eq!(d, &data);
        assert_eq!(r, &rawtrue);
    }

    #[test]
    fn ring_eviction() {
        let mut c = BlockCache::new(MINIMUM_CACHE_SIZE).unwrap();
        // 同じスロットを共有するブロックを追加すると古い方が消える
        c.add_block(1, &block_data(1), false);
        c.add_block(1 + MINIMUM_CACHE_SIZE as u32, &block_data(2), false);
        assert!(c.lookup(1).is_none());
        assert!(c.lookup(1 + MINIMUM_CACHE_SIZE as u32).is_some());
    }
}
