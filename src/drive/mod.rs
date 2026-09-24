//! ドライブ（CD/DVD-ROM）関連。
//!
//! - `mmc`: MMC / ベンダコマンドの CDB 生成（純粋）
//! - `profile`: ドライブ特性テーブル（純粋）
//! - `device`: SCSI 実行抽象（Linux SG_IO / 将来 Windows SPTI）
//! - `dvd`: ドライブの高レベルラッパー（INQUIRY・READ・memdump）

pub mod device;
pub mod dvd;
pub mod mmc;
pub mod profile;
