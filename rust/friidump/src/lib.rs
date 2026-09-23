//! friidump の Rust 移植（進行中）。
//!
//! 上流 C 実装（`libfriidump/`）を増分移植する。まずはハードウェア不要な純粋コア
//! （定数・EDC/LFSR・スクランブル解除）から着手する。
//!
//! 移植方針:
//! - 挙動は C 版と bit-exact を目標とし、ゴールデンベクタで検証する
//! - グローバル可変状態を排除し、状態は構造体が所有する（リファクタ項目 R1）
//! - 失敗は `Result` で伝播する（リファクタ項目 R2）

pub mod constants;
pub mod ecma267;
pub mod error;
pub mod log;
pub mod unscrambler;

pub use error::{Error, Result};
