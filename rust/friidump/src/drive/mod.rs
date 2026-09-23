//! ドライブ（CD/DVD-ROM）関連。
//!
//! - `mmc`: MMC / ベンダコマンドの CDB 生成（純粋）
//! - `profile`: ドライブ特性テーブル（純粋）
//!
//! OS 依存の SG_IO / SPTI 実行層は、実機検証が可能になった時点で追加する。

pub mod mmc;
pub mod profile;
