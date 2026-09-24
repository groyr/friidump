//! ドライブ特性テーブル。
//!
//! vendor / prod_id から
//! memdump 実装・E7 ベースアドレス・読み出しファミリ・既定 method を引く純粋なデータ。

/// ドライブの読み出し方式ファミリ（C 版 `read_family`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFamily {
    /// 汎用（memdump 実装依存）。
    Default,
    /// 0xA13000 固定ベース（GCC-4160N/4240N）。
    HitachiType1,
    /// 0x80000000 回転ベース・4 セクタ E7（GCC-4241N/4242N）。
    HitachiType2,
    /// 0x80000000 固定ベース（従来の MN103 系）。
    HitachiType4,
}

/// ドライブ内メモリダンプの実装種別（C 版の関数ポインタに対応）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemdumpKind {
    /// 0x3C READ BUFFER、2064 バイト配置。
    Vanilla2064,
    /// 0x3C READ BUFFER、2384 バイト配置。
    Vanilla2384,
    /// Hitachi 0xE7、0x80000000 固定ベース。
    Hitachi,
    /// Hitachi MN103S 0xE7、0xA13000 ベース。
    HitachiMn103s,
    /// Lite-On 0x3C READ BUFFER（RS 復号つき）。
    LiteOn,
    /// Renesas。
    Renesas,
}

/// 1 ドライブ機種の特性（C 版 `drive_profile`）。
#[derive(Debug, Clone, Copy)]
pub struct DriveProfile {
    /// INQUIRY のベンダー文字列。
    pub vendor: &'static str,
    /// 製品 ID の部分一致パターン（`None` なら全製品）。
    pub prod_substr: Option<&'static str>,
    /// memdump 実装。
    pub memdump: MemdumpKind,
    /// E7 のベースアドレス（情報用）。
    pub mem_base: u32,
    /// 読み出しファミリ。
    pub family: ReadFamily,
    /// 既定の読み出し method。
    pub def_method: u32,
    /// 既定のメモリダンプコマンド ID。
    pub command: u32,
}

/// ドライブ特性テーブル（上から順に評価。C 版 `PROFILES`）。
pub static PROFILES: &[DriveProfile] = &[
    // --- Hitachi MN103S Type1 (0xA13000, fast method12) ---
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GCC-4160N"),
        memdump: MemdumpKind::HitachiMn103s,
        mem_base: 0xA13000,
        family: ReadFamily::HitachiType1,
        def_method: 12,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GCC-4240N"),
        memdump: MemdumpKind::HitachiMn103s,
        mem_base: 0xA13000,
        family: ReadFamily::HitachiType1,
        def_method: 12,
        command: 2,
    },
    // --- Hitachi MN103S Type2 (0x80000000 rotating base, 4-sector E7; method13 stub) ---
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GCC-4241N"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType2,
        def_method: 13,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GCC-4242N"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType2,
        def_method: 13,
        command: 2,
    },
    // --- Hitachi MN103 (0x80000000 fixed base, method9) ---
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GDR8082N"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GDR8161B"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GDR8162B"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GDR8163B"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GDR8164B"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GCC-4243N"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GCC-4244N"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    DriveProfile {
        vendor: "HL-DT-ST",
        prod_substr: Some("GCC-4247N"),
        memdump: MemdumpKind::Hitachi,
        mem_base: 0x80000000,
        family: ReadFamily::HitachiType4,
        def_method: 9,
        command: 2,
    },
    // --- Lite-On ---
    DriveProfile {
        vendor: "LITE-ON",
        prod_substr: Some("DVDRW LH-18A1H"),
        memdump: MemdumpKind::LiteOn,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 5,
        command: 3,
    },
    DriveProfile {
        vendor: "LITE-ON",
        prod_substr: Some("DVDRW LH-18A1P"),
        memdump: MemdumpKind::LiteOn,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 5,
        command: 3,
    },
    DriveProfile {
        vendor: "LITE-ON",
        prod_substr: Some("DVDRW LH-20A1H"),
        memdump: MemdumpKind::LiteOn,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 5,
        command: 3,
    },
    DriveProfile {
        vendor: "LITE-ON",
        prod_substr: Some("DVDRW LH-20A1P"),
        memdump: MemdumpKind::LiteOn,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 5,
        command: 3,
    },
    // --- Toshiba Samsung ---
    DriveProfile {
        vendor: "TSSTcorp",
        prod_substr: Some("DVD-ROM SH-D162A"),
        memdump: MemdumpKind::Vanilla2384,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 0,
        command: 1,
    },
    DriveProfile {
        vendor: "TSSTcorp",
        prod_substr: Some("DVD-ROM SH-D162B"),
        memdump: MemdumpKind::Vanilla2384,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 0,
        command: 1,
    },
    DriveProfile {
        vendor: "TSSTcorp",
        prod_substr: Some("DVD-ROM SH-D162C"),
        memdump: MemdumpKind::Vanilla2384,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 0,
        command: 1,
    },
    DriveProfile {
        vendor: "TSSTcorp",
        prod_substr: Some("DVD-ROM SH-D162D"),
        memdump: MemdumpKind::Vanilla2384,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 0,
        command: 1,
    },
    // --- Plextor (any product) ---
    DriveProfile {
        vendor: "PLEXTOR",
        prod_substr: None,
        memdump: MemdumpKind::Vanilla2064,
        mem_base: 0,
        family: ReadFamily::Default,
        def_method: 2,
        command: 0,
    },
];

/// vendor / prod_id からドライブ特性を引く（C 版 `drive_profile_lookup`）。
///
/// vendor は完全一致、prod_id は部分一致（`prod_substr` が `None` なら全製品）。
pub fn lookup(vendor: &str, prod_id: &str) -> Option<&'static DriveProfile> {
    PROFILES.iter().find(|p| {
        p.vendor == vendor
            && p.prod_substr
                .map(|sub| prod_id.contains(sub))
                .unwrap_or(true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_mn103s_type1() {
        let p = lookup("HL-DT-ST", "RW/DVD GCC-4240N").unwrap();
        assert_eq!(p.memdump, MemdumpKind::HitachiMn103s);
        assert_eq!(p.mem_base, 0xA13000);
        assert_eq!(p.family, ReadFamily::HitachiType1);
        assert_eq!(p.def_method, 12);
        assert_eq!(p.command, 2);
    }

    #[test]
    fn matches_type2() {
        let p = lookup("HL-DT-ST", "DVD-ROM GDR8082N").unwrap();
        assert_eq!(p.family, ReadFamily::HitachiType4);
        assert_eq!(p.def_method, 9);
    }

    #[test]
    fn matches_plextor_any_product() {
        let p = lookup("PLEXTOR", "DVDR PX-716A").unwrap();
        assert_eq!(p.memdump, MemdumpKind::Vanilla2064);
        assert_eq!(p.def_method, 2);
    }

    #[test]
    fn unknown_drive_returns_none() {
        assert!(lookup("HL-DT-ST", "GCC-9999N").is_none());
        assert!(lookup("NOPE", "GCC-4240N").is_none());
    }
}
