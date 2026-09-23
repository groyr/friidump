//! ログ出力。C 実装 `libfriidump/misc.c` の `_logprintf()` に対応する。
//!
//! 形式は C 版に合わせて `[HH:MM:SS/タグ] レベル: メッセージ` とする。
//! タイムスタンプは現状 UTC。TODO(M4): ローカル時刻（C 版の `localtime_r`）に合わせる。

use std::fmt::Arguments;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// レベル別の接頭辞。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// 通常メッセージ。
    Normal,
    /// 警告。
    Warning,
    /// エラー。
    Error,
    /// デバッグ（`FRIIDUMP_DEBUG` が設定されているときのみ出力）。
    Debug,
}

/// デバッグ出力が有効かどうかを一度だけ判定してキャッシュする。
fn debug_enabled() -> bool {
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static INIT: AtomicBool = AtomicBool::new(false);
    if !INIT.swap(true, Ordering::Relaxed) {
        let on = std::env::var_os("FRIIDUMP_DEBUG").is_some();
        ENABLED.store(on, Ordering::Relaxed);
    }
    ENABLED.load(Ordering::Relaxed)
}

/// `[HH:MM:SS]` 形式のタイムスタンプを返す（UTC）。
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let sod = secs % 86_400;
    format!("{:02}:{:02}:{:02}", sod / 3600, (sod % 3600) / 60, sod % 60)
}

/// ログ本体。`tag` が `None` の場合はタグを省略する。
pub fn write(level: Level, tag: Option<&str>, args: Arguments<'_>) {
    if level == Level::Debug && !debug_enabled() {
        return;
    }
    let ts = timestamp();
    let prefix = match (tag, level) {
        (Some(t), Level::Warning) => format!("[{ts}/{t}] WARNING: "),
        (Some(t), Level::Error) => format!("[{ts}/{t}] ERROR: "),
        (Some(t), Level::Debug) => format!("[{ts}/{t}] DEBUG: "),
        (Some(t), Level::Normal) => format!("[{ts}/{t}] "),
        (None, Level::Warning) => format!("[{ts}] WARNING: "),
        (None, Level::Error) => format!("[{ts}] ERROR: "),
        (None, Level::Debug) => format!("[{ts}] DEBUG: "),
        (None, Level::Normal) => format!("[{ts}] "),
    };
    eprintln!("{prefix}{args}");
}

/// 通常ログを出力する。
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::write($crate::log::Level::Normal, None, format_args!($($arg)*))
    };
}

/// 警告ログを出力する。
#[macro_export]
macro_rules! warning {
    ($($arg:tt)*) => {
        $crate::log::write($crate::log::Level::Warning, None, format_args!($($arg)*))
    };
}

/// エラーログを出力する。
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log::write($crate::log::Level::Error, None, format_args!($($arg)*))
    };
}

/// デバッグログを出力する（`FRIIDUMP_DEBUG` 設定時のみ）。
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::log::write($crate::log::Level::Debug, None, format_args!($($arg)*))
    };
}
