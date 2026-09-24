//! friidump: GameCube / Wii ディスク吸い出し CLI・ライブラリ。
//!
//! 元は Arep による FriiDump（C 実装）。本リポジトリはその Rust 実装で、
//! GameCube / Wii ディスクの読み出し（method11/12 の fast 方式）・スクランブル解除・
//! raw/ISO 出力・resume を行う。
//!
//! - 読み出しは Linux の SG_IO（`/dev/sgN`）を使用する
//! - 失敗は `Result` で伝播し、状態は構造体が所有する（グローバル可変状態なし）
//! - unscrambler/EDC/LFSR・メタデータ・method11/12 はゴールデンベクタと
//!   仮想ドライブのモックで単体テストする

pub mod cache;
pub mod cli;
pub mod constants;
pub mod disc;
pub mod drive;
pub mod dumper;
pub mod ecma267;
pub mod error;
pub mod hasher;
pub mod log;
pub mod metadata;
pub mod read;
pub mod unscrambler;

pub use error::{Error, Result};
