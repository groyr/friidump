//! C 実装から生成したゴールデンベクタとの一致を検証する統合テスト。
//!
//! ベクタは `rust/tools/gen_vectors.c`（C 版 `ecma-267.c` / `unscrambler.c` をリンク）で
//! 生成し、`tests/vectors/` に固定している。

use friidump::ecma267::{edc_calc, Lfsr};
use friidump::unscrambler::Unscrambler;
use std::path::PathBuf;

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors")
}

fn decode_hex(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    assert!(bytes.len() & 1 == 0, "16 進文字列の長さが奇数です");
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let val = |c: u8| -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => panic!("不正な 16 進文字: {}", c as char),
        }
    };
    for chunk in bytes.chunks(2) {
        out.push((val(chunk[0]) << 4) | val(chunk[1]));
    }
    out
}

#[test]
fn edc_matches_c() {
    let text = std::fs::read_to_string(vectors_dir().join("edc.txt")).unwrap();
    let mut count = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut it = line.split_whitespace();
        let len: usize = it.next().unwrap().parse().unwrap();
        let data = decode_hex(it.next().unwrap());
        let expected = u32::from_str_radix(it.next().unwrap(), 16).unwrap();
        assert_eq!(data.len(), len);
        assert_eq!(edc_calc(0, &data), expected, "EDC 不一致 (len={len})");
        count += 1;
    }
    assert!(count >= 4, "EDC ベクタが不足しています");
}

#[test]
fn lfsr_matches_c() {
    let text = std::fs::read_to_string(vectors_dir().join("lfsr.txt")).unwrap();
    let mut count = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut it = line.split_whitespace();
        let seed = u16::from_str_radix(it.next().unwrap(), 16).unwrap();
        let expected = decode_hex(it.next().unwrap());
        let mut lfsr = Lfsr::new(seed);
        let got: Vec<u8> = (0..expected.len()).map(|_| lfsr.next_byte()).collect();
        assert_eq!(got, expected, "LFSR 系列不一致 (seed={seed:04x})");
        count += 1;
    }
    assert!(count >= 5, "LFSR ベクタが不足しています");
}

#[test]
fn unscramble_matches_c() {
    let text = std::fs::read_to_string(vectors_dir().join("unscramble.txt")).unwrap();
    let mut count = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut it = line.split_whitespace();
        let seed = u16::from_str_radix(it.next().unwrap(), 16).unwrap();
        let sector_no: u32 = it.next().unwrap().parse().unwrap();
        let disctype: u8 = it.next().unwrap().parse().unwrap();
        let raw_vec = decode_hex(it.next().unwrap());
        let iso = decode_hex(it.next().unwrap());

        let mut raw: [u8; friidump::constants::RAW_BLOCK_SIZE] = raw_vec.try_into().unwrap();
        let mut out = [0u8; friidump::constants::BLOCK_SIZE];

        let mut u = Unscrambler::new();
        u.set_disctype(disctype);
        u.set_bruteforce(true);
        assert!(
            u.unscramble_16sectors(sector_no, &mut raw, &mut out)
                .unwrap(),
            "スクランブル解除に失敗 (seed={seed:04x}, sector={sector_no})"
        );
        assert_eq!(
            &out[..],
            &iso[..],
            "ISO 出力不一致 (seed={seed:04x}, sector={sector_no})"
        );
        count += 1;
    }
    assert!(count >= 2, "unscramble ベクタが不足しています");
}
