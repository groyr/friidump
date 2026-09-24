//! GameCube / Wii ディスクのジオメトリ定数。
//!
//! 元とした C 実装の定数定義に対応する。

/// スクランブル済み 1 セクタのサイズ（バイト）。
pub const RAW_SECTOR_SIZE: usize = 2064;

/// スクランブル解除済み 1 セクタのサイズ（バイト）。
pub const SECTOR_SIZE: usize = 2048;

/// 1 ブロックあたりのセクタ数。
pub const SECTORS_PER_BLOCK: usize = 16;

/// スクランブル済み 1 ブロックのサイズ（バイト）。
pub const RAW_BLOCK_SIZE: usize = RAW_SECTOR_SIZE * SECTORS_PER_BLOCK;

/// スクランブル解除済み 1 ブロックのサイズ（バイト）。
pub const BLOCK_SIZE: usize = SECTOR_SIZE * SECTORS_PER_BLOCK;

/// GameCube ディスクの総セクタ数（712880）。
pub const DISC_GAMECUBE_SECTORS_NO: u32 = 0x0AE0B0;

/// Wii 単層ディスクの総セクタ数（2294912）。
pub const DISC_WII_SECTORS_NO_SL: u32 = 0x230480;

/// Wii 二層ディスクの総セクタ数（4155840）。
pub const DISC_WII_SECTORS_NO_DL: u32 = 0x3F69C0;
