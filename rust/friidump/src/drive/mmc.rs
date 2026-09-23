//! MMC / ベンダコマンドの CDB（コマンド・デスクリプタ・ブロック）生成。
//!
//! C 実装 `libfriidump/dvd_drive.c` および各 memdump 実装のコマンド組み立てを移植する。
//! 純粋関数のみで、デバイス I/O には依存しない。

/// READ(12) のオペコード。
pub const MMC_READ_12: u8 = 0xA8;
/// INQUIRY のオペコード。
pub const SPC_INQUIRY: u8 = 0x12;
/// READ BUFFER(0x3C)。
pub const MMC_READ_BUFFER: u8 = 0x3C;
/// ベンダ固有メモリダンプ（0xE7）。
pub const MMC_VENDOR_E7: u8 = 0xE7;

/// 転送方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// デバイスからホスト（受信）。
    FromDevice,
    /// ホストからデバイス（送信）。
    ToDevice,
}

/// 送受信データを伴う SCSI コマンド。
#[derive(Debug, Clone)]
pub struct Command {
    /// 12 バイトの CDB。
    pub cdb: [u8; 12],
    /// データ転送方向。
    pub direction: Direction,
    /// データバッファ長（受信時は確保すべき長さ、送信時は `data` の長さ）。
    pub length: usize,
    /// 送信データ（`ToDevice` のときのみ）。
    pub data: Vec<u8>,
}

impl Command {
    fn new(cdb: [u8; 12], direction: Direction, length: usize) -> Self {
        Self {
            cdb,
            direction,
            length,
            data: Vec::new(),
        }
    }
}

/// INQUIRY（C 版 `dvd_get_drive_info`）。
pub fn inquiry(alloc_len: u8) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = SPC_INQUIRY;
    cdb[4] = alloc_len;
    Command::new(cdb, Direction::FromDevice, alloc_len as usize)
}

/// READ(12) を FUA 付きで発行する（C 版 `dvd_read_sector_dummy`）。応答データは使わない。
pub fn read12_dummy(sector: u32, sectors: u32) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = MMC_READ_12;
    cdb[1] = 0x08; // FUA ビット
    cdb[2..6].copy_from_slice(&sector.to_be_bytes());
    cdb[6..10].copy_from_slice(&sectors.to_be_bytes());
    Command::new(cdb, Direction::FromDevice, 0)
}

/// READ(12) を streaming ビット付きで発行する（C 版 `dvd_read_sector_streaming`）。
///
/// 転送長は常に 0x10（16 セクタ）。
pub fn read12_streaming(sector: u32) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = MMC_READ_12;
    cdb[2..6].copy_from_slice(&sector.to_be_bytes());
    cdb[9] = 0x10;
    cdb[10] = 0x80; // STREAMING ビット
    Command::new(cdb, Direction::FromDevice, 0)
}

/// READ(12) を任意セクタ数・streaming ビット付きで発行する（C 版 `dvd_read_streaming`）。
pub fn read12_streaming_n(sector: u32, sectors: u32, buf_len: usize) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = MMC_READ_12;
    cdb[2..6].copy_from_slice(&sector.to_be_bytes());
    cdb[6..10].copy_from_slice(&sectors.to_be_bytes());
    cdb[10] = 0x80;
    Command::new(cdb, Direction::FromDevice, buf_len)
}

/// READ(12) を FUA 付き・転送長 0 で発行し、キャッシュを流す（C 版 `dvd_flush_cache_READ12`）。
pub fn flush_cache_read12(sector: u32) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = MMC_READ_12;
    cdb[1] = 0x08;
    cdb[2..6].copy_from_slice(&sector.to_be_bytes());
    Command::new(cdb, Direction::FromDevice, 0)
}

/// START STOP UNIT（C 版 `dvd_stop_unit`）。`start=false` で回転停止。
pub fn stop_unit(start: bool) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = 0x1B;
    cdb[4] = u8::from(start);
    Command::new(cdb, Direction::FromDevice, 0)
}

/// SET CD SPEED（C 版 `dvd_set_speed`）。
pub fn set_speed(speed: u32) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = 0xBB;
    cdb[2] = ((speed >> 8) & 0xFF) as u8;
    cdb[3] = (speed & 0xFF) as u8;
    Command::new(cdb, Direction::FromDevice, 0)
}

/// READ CAPACITY(0x52)（C 版 `dvd_get_size`）。応答は `parse_read_capacity` で解析する。
pub fn read_capacity() -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = 0x52;
    cdb[1] = 0x01;
    cdb[5] = 0x01;
    cdb[8] = 0x22;
    Command::new(cdb, Direction::FromDevice, 0x22)
}

/// READ DVD STRUCTURE(0xAD)（C 版 `dvd_get_layerbreak`）。応答は `parse_layerbreak` で解析する。
pub fn read_dvd_structure() -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = 0xAD;
    cdb[8] = 0x08;
    cdb[9] = 0x04;
    Command::new(cdb, Direction::FromDevice, 2052)
}

/// SET STREAMING(0xB6)（C 版 `dvd_set_streaming`）。28 バイトの送信データを伴う。
pub fn set_streaming(speed: u32) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = 0xB6;
    cdb[10] = 28;

    let mut data = vec![0u8; 28];
    data[8..12].copy_from_slice(&[0xFF; 4]);
    data[12..16].copy_from_slice(&speed.to_be_bytes());
    data[16..20].copy_from_slice(&1000u32.to_be_bytes());
    data[20..24].copy_from_slice(&speed.to_be_bytes());
    data[24..28].copy_from_slice(&1000u32.to_be_bytes());

    let mut cmd = Command::new(cdb, Direction::ToDevice, 28);
    cmd.data = data;
    cmd
}

/// Hitachi 系のベンダ固有メモリダンプ（C 版 `hitachi_dvd_dump_memblock`、0xE7）。
pub fn e7_memdump(offset: u32, block_size: u16) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = MMC_VENDOR_E7;
    cdb[1] = b'H';
    cdb[2] = b'I';
    cdb[3] = b'T';
    cdb[4] = 0x01; // MCU メモリ読み出しサブコマンド
    cdb[6..10].copy_from_slice(&offset.to_be_bytes());
    cdb[10] = ((block_size & 0xFF00) >> 8) as u8;
    cdb[11] = (block_size & 0x00FF) as u8;
    Command::new(cdb, Direction::FromDevice, block_size as usize)
}

/// vanilla 2064 のメモリダンプ（C 版 `vanilla_2064_dvd_dump_memblock`、0x3C READ BUFFER）。
pub fn read_buffer_vanilla2064(offset: u32, block_size: u32) -> Command {
    let mut cdb = [0u8; 12];
    cdb[0] = MMC_READ_BUFFER;
    cdb[1] = 0x02;
    cdb[2] = 0x00;
    cdb[3] = ((offset >> 16) & 0xFF) as u8;
    cdb[4] = ((offset >> 8) & 0xFF) as u8;
    cdb[5] = (offset & 0xFF) as u8;
    cdb[6] = ((block_size >> 16) & 0xFF) as u8;
    cdb[7] = ((block_size >> 8) & 0xFF) as u8;
    cdb[8] = (block_size & 0xFF) as u8;
    Command::new(cdb, Direction::FromDevice, block_size as usize)
}

/// vanilla 2384 / Lite-On のメモリダンプ（C 版 `vanilla_2384_dvd_dump_memblock` /
/// `liteon_dvd_dump_memblock`、0x3C READ BUFFER）。
///
/// 内部では 2064 バイト配置を 0x950（2384）バイト配置へ変換するため、
/// アドレスと長さは `(block_size / 2064) * 0x950` でスケールされる。
/// `liteon=true` のときサブコマンドが Lite-On 用（byte1=0x01, byte2=0x01）になる。
pub fn read_buffer_2384(offset: u32, block_size: u32, liteon: bool) -> Command {
    let raw_block_size = (block_size / 2064) * 0x950;
    let raw_offset = (offset / 2064) * 0x950;

    let mut cdb = [0u8; 12];
    cdb[0] = MMC_READ_BUFFER;
    if liteon {
        cdb[1] = 0x01;
        cdb[2] = 0x01;
    } else {
        cdb[1] = 0x02;
        cdb[2] = 0x00;
    }
    cdb[3] = ((raw_offset >> 16) & 0xFF) as u8;
    cdb[4] = ((raw_offset >> 8) & 0xFF) as u8;
    cdb[5] = (raw_offset & 0xFF) as u8;
    cdb[6] = ((raw_block_size >> 16) & 0xFF) as u8;
    cdb[7] = ((raw_block_size >> 8) & 0xFF) as u8;
    cdb[8] = (raw_block_size & 0xFF) as u8;
    Command::new(cdb, Direction::FromDevice, raw_block_size as usize)
}

/// READ CAPACITY の応答から総セクタ数を取り出す（C 版 `dvd_get_size` の後処理）。
///
/// C 版は未初期化の出力変数をシフトする脆弱な実装だったため、ここでは
/// バイト列から明示的にビッグエンディアンで構築する（リファクタ項目 R7）。
pub fn parse_read_capacity(buf: &[u8]) -> u32 {
    u32::from_be_bytes([buf[0x18], buf[0x19], buf[0x1a], buf[0x1b]])
}

/// READ DVD STRUCTURE の応答から layer break を取り出す（C 版 `dvd_get_layerbreak` の後処理）。
///
/// C 版の未初期化変数シフトを是正し、バイト列から明示的に構築する（リファクタ項目 R7）。
pub fn parse_layerbreak(buf: &[u8]) -> u32 {
    let v = u32::from_be_bytes([0, buf[0x11], buf[0x12], buf[0x13]]);
    if v > 0 {
        v - 0x30000 + 1
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inquiry_cdb() {
        let c = inquiry(36);
        assert_eq!(c.cdb[0], 0x12);
        assert_eq!(c.cdb[4], 36);
        assert_eq!(c.direction, Direction::FromDevice);
        assert_eq!(c.length, 36);
    }

    #[test]
    fn read12_dummy_has_fua_and_be_fields() {
        let c = read12_dummy(0x00123456, 16);
        assert_eq!(c.cdb[0], 0xA8);
        assert_eq!(c.cdb[1], 0x08);
        assert_eq!(&c.cdb[2..6], &[0x00, 0x12, 0x34, 0x56]);
        assert_eq!(&c.cdb[6..10], &[0, 0, 0, 16]);
        assert_eq!(c.length, 0);
    }

    #[test]
    fn read12_streaming_fixed_16() {
        let c = read12_streaming(0x00000020);
        assert_eq!(c.cdb[0], 0xA8);
        assert_eq!(&c.cdb[2..6], &[0, 0, 0, 0x20]);
        assert_eq!(c.cdb[9], 0x10);
        assert_eq!(c.cdb[10], 0x80);
    }

    #[test]
    fn set_streaming_payload() {
        let c = set_streaming(0x0000_0ABC);
        assert_eq!(c.cdb[0], 0xB6);
        assert_eq!(c.cdb[10], 28);
        assert_eq!(c.direction, Direction::ToDevice);
        assert_eq!(c.data.len(), 28);
        assert_eq!(&c.data[8..12], &[0xFF; 4]);
        assert_eq!(&c.data[12..16], &[0, 0, 0x0A, 0xBC]);
        assert_eq!(&c.data[16..20], &1000u32.to_be_bytes());
    }

    #[test]
    fn e7_memdump_cdb() {
        let c = e7_memdump(0xA13000, 12);
        assert_eq!(c.cdb[0], 0xE7);
        assert_eq!(&c.cdb[1..5], b"HIT\x01");
        assert_eq!(&c.cdb[6..10], &[0x00, 0xA1, 0x30, 0x00]);
        assert_eq!(c.cdb[10], 0x00);
        assert_eq!(c.cdb[11], 12);
    }

    #[test]
    fn read_buffer_2384_scales_offset_and_length() {
        // 16 セクタ分 (33024) を要求 → 0x950 * 16 = 0x9500
        let c = read_buffer_2384(0, 16 * 2064, false);
        assert_eq!(c.cdb[0], 0x3C);
        assert_eq!(c.cdb[1], 0x02);
        assert_eq!(&c.cdb[6..9], &[0x00, 0x95, 0x00]);
        // Lite-On はサブコマンドが異なる
        let l = read_buffer_2384(0, 16 * 2064, true);
        assert_eq!(&l.cdb[1..3], &[0x01, 0x01]);
    }

    #[test]
    fn parse_read_capacity_be() {
        let mut buf = [0u8; 0x22];
        buf[0x18..0x1c].copy_from_slice(&712_880u32.to_be_bytes());
        assert_eq!(parse_read_capacity(&buf), 712_880);
    }

    #[test]
    fn parse_layerbreak_applies_offset() {
        let mut buf = [0u8; 2052];
        // 生値 0x21_0000 相当 → -0x30000 +1
        let raw: u32 = 0x21_0000;
        buf[0x11] = (raw >> 16) as u8;
        buf[0x12] = (raw >> 8) as u8;
        buf[0x13] = raw as u8;
        assert_eq!(parse_layerbreak(&buf), raw - 0x30000 + 1);
        // 0 はそのまま
        assert_eq!(parse_layerbreak(&[0u8; 2052]), 0);
    }
}
