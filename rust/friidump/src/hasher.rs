//! CRC32 / MD5 / SHA-1 のマルチハッシュ。
//!
//! C 実装 `libmultihash`（`multihash.c`）の CLI が表示する 3 種を Rust クレートで置き換える。
//! C 版と同一の値（CRC-32/ISO-HDLC、MD5、SHA-1）を生成することをゴールデンベクタで検証する。
//!
//! 参考: C 版 `libmultihash` の MD4 / ED2K は CLI では未使用（コメントアウト）のため移植しない。

use md5::{Digest, Md5};
use sha1::Sha1;

/// 計算済みのハッシュ文字列（いずれも小文字 16 進）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digests {
    /// CRC32（8 桁）。
    pub crc32: String,
    /// MD5（32 桁）。
    pub md5: String,
    /// SHA-1（40 桁）。
    pub sha1: String,
}

/// 3 種のハッシュを同時に更新するマルチハッシュ。
pub struct MultiHash {
    crc: crc32fast::Hasher,
    md5: Md5,
    sha1: Sha1,
}

impl MultiHash {
    /// 新しいマルチハッシュを生成する（C 版 `multihash_init`）。
    pub fn new() -> Self {
        Self {
            crc: crc32fast::Hasher::new(),
            md5: Md5::new(),
            sha1: Sha1::new(),
        }
    }

    /// データを追加する（C 版 `multihash_update`）。
    pub fn update(&mut self, data: &[u8]) {
        self.crc.update(data);
        self.md5.update(data);
        self.sha1.update(data);
    }

    /// 計算を確定し、ハッシュ文字列を返す（C 版 `multihash_finish`）。
    pub fn finish(self) -> Digests {
        let crc = self.crc.finalize();
        let md5 = self.md5.finalize();
        let sha1 = self.sha1.finalize();
        Digests {
            crc32: format!("{crc:08x}"),
            md5: to_hex(&md5),
            sha1: to_hex(&sha1),
        }
    }
}

impl Default for MultiHash {
    fn default() -> Self {
        Self::new()
    }
}

/// バイト列を小文字 16 進文字列にする。
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
