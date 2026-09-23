//! friidump-rs: Rust 移植の CLI エントリ。
//!
//! M0 時点では純粋コア（スクランブル解除）のみ実装済み。CLI は M4 で実装する。

fn main() {
    eprintln!(
        "friidump-rs {} (Rust 移植・進行中: 純粋コアのみ)",
        env!("CARGO_PKG_VERSION")
    );
    std::process::exit(0);
}
