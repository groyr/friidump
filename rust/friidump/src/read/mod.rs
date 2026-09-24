//! 読み出し方式（method0-13）の識別と、読み出しループ用パラメータの計算。
//!
//! C 実装 `libfriidump/disc.c` の `disc_set_read_method` のうち、方式 ID の扱いと
//! `sec_disc`/`sec_mem`/`max_cnt`/`max_blk` の算出を移植する。デバイス I/O には依存しない。

use crate::constants::SECTORS_PER_BLOCK;
use crate::error::{Error, Result};

/// 読み出し方式（C 版 method0-13）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMethod {
    /// method0: 通常 READ + memdump。
    M0,
    /// method1: 非ストリーミング。
    M1,
    /// method2: 非ストリーミング。
    M2,
    /// method3: 非ストリーミング。
    M3,
    /// method4: ストリーミング。
    M4,
    /// method5: ストリーミング。
    M5,
    /// method6: ストリーミング。
    M6,
    /// method7: Hitachi。
    M7,
    /// method8: Hitachi。
    M8,
    /// method9: Hitachi。
    M9,
    /// method10: Hitachi 0xA13000。
    M10,
    /// method11: fast 方式。
    M11,
    /// method12: fast + raw マスター + 全ブロック EDC。
    M12,
    /// method13: Hitachi Type2（未実装）。
    M13,
}

impl ReadMethod {
    /// 全方式。
    pub const ALL: [ReadMethod; 14] = [
        ReadMethod::M0,
        ReadMethod::M1,
        ReadMethod::M2,
        ReadMethod::M3,
        ReadMethod::M4,
        ReadMethod::M5,
        ReadMethod::M6,
        ReadMethod::M7,
        ReadMethod::M8,
        ReadMethod::M9,
        ReadMethod::M10,
        ReadMethod::M11,
        ReadMethod::M12,
        ReadMethod::M13,
    ];

    /// 方式 ID から変換する（0-13 以外は `None`）。
    pub fn from_id(id: i32) -> Option<Self> {
        match id {
            0 => Some(ReadMethod::M0),
            1 => Some(ReadMethod::M1),
            2 => Some(ReadMethod::M2),
            3 => Some(ReadMethod::M3),
            4 => Some(ReadMethod::M4),
            5 => Some(ReadMethod::M5),
            6 => Some(ReadMethod::M6),
            7 => Some(ReadMethod::M7),
            8 => Some(ReadMethod::M8),
            9 => Some(ReadMethod::M9),
            10 => Some(ReadMethod::M10),
            11 => Some(ReadMethod::M11),
            12 => Some(ReadMethod::M12),
            13 => Some(ReadMethod::M13),
            _ => None,
        }
    }

    /// 方式 ID を返す。
    pub fn id(self) -> u32 {
        match self {
            ReadMethod::M0 => 0,
            ReadMethod::M1 => 1,
            ReadMethod::M2 => 2,
            ReadMethod::M3 => 3,
            ReadMethod::M4 => 4,
            ReadMethod::M5 => 5,
            ReadMethod::M6 => 6,
            ReadMethod::M7 => 7,
            ReadMethod::M8 => 8,
            ReadMethod::M9 => 9,
            ReadMethod::M10 => 10,
            ReadMethod::M11 => 11,
            ReadMethod::M12 => 12,
            ReadMethod::M13 => 13,
        }
    }

    /// ストリーミング方式（method4-6）かどうか。
    pub fn is_streaming(self) -> bool {
        matches!(self, ReadMethod::M4 | ReadMethod::M5 | ReadMethod::M6)
    }

    /// 既定の `sec_disc`/`sec_mem`（ストリーミングは 27、それ以外は 16）。
    pub fn default_sec(self) -> u32 {
        if self.is_streaming() {
            27
        } else {
            16
        }
    }
}

/// `sec_disc` の有効範囲を判定する（C 版 `init_range`、1-100）。
pub fn resolve_sec_disc(v: i64) -> Option<u32> {
    if (1..=100).contains(&v) {
        Some(v as u32)
    } else {
        None
    }
}

/// `sec_mem` の有効範囲を判定する（C 版 `init_range`、16-100）。
pub fn resolve_sec_mem(v: i64) -> Option<u32> {
    if (16..=100).contains(&v) {
        Some(v as u32)
    } else {
        None
    }
}

/// 読み出しループ用のパラメータ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadParams {
    /// ディスクから要求するセクタ数。
    pub sec_disc: u32,
    /// メモリから取得するセクタ数。
    pub sec_mem: u32,
    /// 追加読み出し回数。
    pub max_cnt: u32,
    /// 1 回で得られるブロック数。
    pub max_blk: u32,
}

/// 方式と（任意の）`sec_disc`/`sec_mem` からパラメータを計算する（C 版 `disc_set_read_method`）。
pub fn compute_params(
    method: ReadMethod,
    sec_disc: Option<u32>,
    sec_mem: Option<u32>,
) -> Result<ReadParams> {
    let sec_disc = sec_disc.unwrap_or_else(|| method.default_sec());
    let sec_mem = sec_mem.unwrap_or_else(|| method.default_sec());
    if sec_mem == 0 {
        return Err(Error::InvalidArgument("sec_mem が 0 です".to_string()));
    }

    let spb = SECTORS_PER_BLOCK as u32;
    let deviation = sec_mem % spb;
    let mut counter = 0u32;
    if deviation > 3 {
        let mut cnt1 = deviation;
        loop {
            cnt1 += deviation;
            counter += 1;
            if cnt1 % spb <= 1 {
                break;
            }
        }
    }
    let max_cnt = counter;
    let n = sec_mem * (max_cnt + 1);
    let max_blk = (n - (n % spb)) / spb;

    Ok(ReadParams {
        sec_disc,
        sec_mem,
        max_cnt,
        max_blk,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_ids_roundtrip() {
        for m in ReadMethod::ALL {
            assert_eq!(ReadMethod::from_id(m.id() as i32), Some(m));
        }
        assert_eq!(ReadMethod::from_id(14), None);
        assert_eq!(ReadMethod::from_id(-1), None);
    }

    #[test]
    fn streaming_defaults_27_others_16() {
        assert_eq!(ReadMethod::M4.default_sec(), 27);
        assert_eq!(ReadMethod::M5.default_sec(), 27);
        assert_eq!(ReadMethod::M6.default_sec(), 27);
        assert_eq!(ReadMethod::M12.default_sec(), 16);
        assert_eq!(ReadMethod::M0.default_sec(), 16);
    }

    #[test]
    fn known_params() {
        // sec_mem=16 → max_cnt=0, max_blk=1
        let p = compute_params(ReadMethod::M12, None, None).unwrap();
        assert_eq!((p.sec_mem, p.max_cnt, p.max_blk), (16, 0, 1));
        // sec_mem=27 → max_cnt=2, max_blk=5
        let p = compute_params(ReadMethod::M4, None, None).unwrap();
        assert_eq!((p.sec_mem, p.max_cnt, p.max_blk), (27, 2, 5));
        // sec_mem=32 → max_cnt=0, max_blk=2
        let p = compute_params(ReadMethod::M0, None, Some(32)).unwrap();
        assert_eq!((p.sec_mem, p.max_cnt, p.max_blk), (32, 0, 2));
        // sec_mem=20 → max_cnt=3, max_blk=5
        let p = compute_params(ReadMethod::M0, None, Some(20)).unwrap();
        assert_eq!((p.sec_mem, p.max_cnt, p.max_blk), (20, 3, 5));
        // sec_mem=100 → max_cnt=3, max_blk=25
        let p = compute_params(ReadMethod::M0, None, Some(100)).unwrap();
        assert_eq!((p.sec_mem, p.max_cnt, p.max_blk), (100, 3, 25));
        // deviation<=3 は counter=0
        let p = compute_params(ReadMethod::M0, None, Some(17)).unwrap();
        assert_eq!((p.sec_mem, p.max_cnt, p.max_blk), (17, 0, 1));
    }

    #[test]
    fn range_resolution() {
        assert_eq!(resolve_sec_disc(1), Some(1));
        assert_eq!(resolve_sec_disc(100), Some(100));
        assert_eq!(resolve_sec_disc(0), None);
        assert_eq!(resolve_sec_disc(101), None);
        assert_eq!(resolve_sec_mem(16), Some(16));
        assert_eq!(resolve_sec_mem(15), None);
        assert_eq!(resolve_sec_mem(101), None);
    }
}
