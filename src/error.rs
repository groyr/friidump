//! エラー型。C 実装の `exit()` / `error()` 直呼びを置き換える。
//!
//! 移植方針（リファクタ項目 R2）: 失敗は `Result` で伝播し、最終的な表示と
//! 終了コードの決定は `main` に集約する。

use std::fmt;

/// friidump 全体のエラー型。
#[derive(Debug)]
pub enum Error {
    /// 引数・設定値が不正。
    InvalidArgument(String),
    /// 入出力エラー。
    Io(std::io::Error),
    /// 対象外・未実装の機能。
    Unsupported(String),
    /// スクランブル解除に失敗（EDC 不一致など）。
    Unscramble(String),
    /// 指定セクタの seed が見つからなかった。
    SeedNotFound(u32),
    /// その他のエラー。
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidArgument(s) => write!(f, "引数が不正です: {s}"),
            Error::Io(e) => write!(f, "入出力エラー: {e}"),
            Error::Unsupported(s) => write!(f, "未対応です: {s}"),
            Error::Unscramble(s) => write!(f, "スクランブル解除に失敗しました: {s}"),
            Error::SeedNotFound(sec) => write!(f, "セクタ {sec} の seed が見つかりません"),
            Error::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// friidump の結果型。
pub type Result<T> = std::result::Result<T, Error>;
