//! コマンドライン引数の解析（`getopt_long` 互換）。
//!
//! `clap` 等は使わず、armv6 ビルドの依存を増やさないよう自前で解析する。
//! `--methodN=req,exp` のような省略可能引数にも対応する。

use crate::error::{Error, Result};
use std::path::PathBuf;

/// 解析済みオプション。
#[derive(Debug, Default, Clone)]
pub struct Options {
    /// `-d/--device`。
    pub device: Option<String>,
    /// `-a/--autodump`。
    pub autodump: bool,
    /// `-g/--gui`。
    pub gui: bool,
    /// `-r/--raw`。
    pub raw_out: Option<PathBuf>,
    /// `-i/--iso`。
    pub iso_out: Option<PathBuf>,
    /// `-u/--unscramble`。
    pub raw_in: Option<PathBuf>,
    /// `-s/--resume`（`-a/-A` でも有効化）。
    pub resume: bool,
    /// 読み出し method（未指定は None → ドライブ既定）。
    pub dump_method: Option<i32>,
    /// `-c/--command`（未指定は -1）。
    pub command: i64,
    /// `-t/--startsector`。
    pub start_sector: Option<u32>,
    /// `-e/--stopsector`（exclusive）。
    pub end_sector: Option<u32>,
    /// `-S/--size`。
    pub sectors_no: Option<u32>,
    /// `-x/--speed`。
    pub speed: Option<u32>,
    /// `-T/--type`。
    pub disctype: Option<u32>,
    /// `--methodN=req,exp` の req。
    pub sec_disc: Option<u32>,
    /// `--methodN=req,exp` の exp。
    pub sec_mem: Option<u32>,
    /// `-H/--nohash`。
    pub no_hashing: bool,
    /// `-p/--stop`。
    pub stop_unit: bool,
    /// `-A/--allmethods`。
    pub allmethods: bool,
    /// `-h/--help`。
    pub help: bool,
}

/// 引数を解析する。
pub fn parse(args: &[String]) -> Result<Options> {
    let mut o = Options {
        command: -1,
        ..Default::default()
    };
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if arg == "-h" || arg == "--help" {
            o.help = true;
            continue;
        }
        if arg == "--autodump" {
            o.autodump = true;
            o.resume = true;
            continue;
        }
        if arg == "--gui" {
            o.gui = true;
            continue;
        }
        if arg == "--nohash" {
            o.no_hashing = true;
            continue;
        }
        if arg == "--resume" {
            o.resume = true;
            continue;
        }
        if arg == "--stop" {
            o.stop_unit = true;
            continue;
        }
        if arg == "--allmethods" {
            o.allmethods = true;
            o.resume = true;
            continue;
        }
        if let Some((name, val)) = split_long(arg) {
            match name {
                "device" | "raw" | "iso" | "unscramble" | "command" | "startsector"
                | "stopsector" | "size" | "speed" | "type" => {
                    let v = val.or_else(|| take_next(args, &mut i)).ok_or_else(|| {
                        Error::InvalidArgument(format!("--{name} には引数が必要です"))
                    })?;
                    apply_value(&mut o, name, &v)?;
                }
                n if n.starts_with("method") => {
                    let id: i32 = n[6..]
                        .parse()
                        .map_err(|_| Error::InvalidArgument(format!("不正なオプション: --{n}")))?;
                    o.dump_method = Some(id);
                    if let Some(spec) = val {
                        parse_sec_spec(&mut o, &spec)?;
                    }
                }
                _ => return Err(Error::InvalidArgument(format!("不明なオプション: {arg}"))),
            }
            continue;
        }
        if let Some(short) = arg.strip_prefix('-') {
            if short.is_empty() {
                return Err(Error::InvalidArgument("不正な引数: -".to_string()));
            }
            for (idx, c) in short.char_indices() {
                match c {
                    'd' | 'r' | 'i' | 'u' | 'c' | 't' | 'e' | 'S' | 'x' | 'T' => {
                        // 引数: 同一トークンの残り、または次トークン
                        let rest = &short[idx + 1..];
                        let v = if !rest.is_empty() {
                            rest.to_string()
                        } else {
                            take_next(args, &mut i).ok_or_else(|| {
                                Error::InvalidArgument(format!("-{c} には引数が必要です"))
                            })?
                        };
                        let name = short_name(c);
                        apply_value(&mut o, name, &v)?;
                        break;
                    }
                    'a' => {
                        o.autodump = true;
                        o.resume = true;
                    }
                    'g' => o.gui = true,
                    'H' => o.no_hashing = true,
                    's' => o.resume = true,
                    'p' => o.stop_unit = true,
                    'A' => {
                        o.allmethods = true;
                        o.resume = true;
                    }
                    'J' => o.dump_method = Some(10),
                    'K' => o.dump_method = Some(11),
                    'L' => o.dump_method = Some(12),
                    'M' => o.dump_method = Some(13),
                    '0'..='6' => {
                        o.dump_method = Some((c as u8 - b'0') as i32);
                        // 同トークン残りが "16,16" 等なら req,exp
                        let rest = &short[idx + 1..];
                        if !rest.is_empty() {
                            parse_sec_spec(&mut o, rest.trim_start_matches('='))?;
                        }
                    }
                    '7' | '8' | '9' => o.dump_method = Some((c as u8 - b'0') as i32),
                    _ => return Err(Error::InvalidArgument(format!("不明なオプション: -{c}"))),
                }
            }
            continue;
        }
        // 位置引数は無視（C 版も警告して無視する）
    }
    Ok(o)
}

fn split_long(arg: &str) -> Option<(&str, Option<String>)> {
    let body = arg.strip_prefix("--")?;
    match body.split_once('=') {
        Some((n, v)) => Some((n, Some(v.to_string()))),
        None => Some((body, None)),
    }
}

fn take_next(args: &[String], i: &mut usize) -> Option<String> {
    if *i < args.len() {
        let v = args[*i].clone();
        *i += 1;
        Some(v)
    } else {
        None
    }
}

fn short_name(c: char) -> &'static str {
    match c {
        'd' => "device",
        'r' => "raw",
        'i' => "iso",
        'u' => "unscramble",
        'c' => "command",
        't' => "startsector",
        'e' => "stopsector",
        'S' => "size",
        'x' => "speed",
        'T' => "type",
        _ => unreachable!(),
    }
}

fn apply_value(o: &mut Options, name: &str, v: &str) -> Result<()> {
    let num = |v: &str| -> Result<u32> {
        v.trim()
            .parse::<i64>()
            .map_err(|_| Error::InvalidArgument(format!("数値が不正です: {v}")))
            .and_then(|n| {
                u32::try_from(n)
                    .map_err(|_| Error::InvalidArgument(format!("数値が範囲外です: {v}")))
            })
    };
    match name {
        "device" => o.device = Some(v.to_string()),
        "raw" => o.raw_out = Some(PathBuf::from(v)),
        "iso" => o.iso_out = Some(PathBuf::from(v)),
        "unscramble" => o.raw_in = Some(PathBuf::from(v)),
        "command" => {
            o.command = v
                .trim()
                .parse()
                .map_err(|_| Error::InvalidArgument(format!("数値が不正です: {v}")))?;
        }
        "startsector" => o.start_sector = Some(num(v)?),
        "stopsector" => o.end_sector = Some(num(v)?),
        "size" => o.sectors_no = Some(num(v)?),
        "speed" => o.speed = Some(num(v)?),
        "type" => {
            let t = num(v)?;
            if t > 3 {
                return Err(Error::InvalidArgument("-T は 0-3".to_string()));
            }
            o.disctype = Some(t);
        }
        _ => return Err(Error::InvalidArgument(format!("不明なオプション: {name}"))),
    }
    Ok(())
}

fn parse_sec_spec(o: &mut Options, spec: &str) -> Result<()> {
    let mut it = spec.split(',');
    let req = it.next().unwrap_or("").trim();
    let exp = it.next().map(|s| s.trim());
    if let Some(exp) = exp {
        o.sec_disc = Some(
            req.parse()
                .map_err(|_| Error::InvalidArgument(format!("req が不正: {req}")))?,
        );
        o.sec_mem = Some(
            exp.parse()
                .map_err(|_| Error::InvalidArgument(format!("exp が不正: {exp}")))?,
        );
    } else if !req.is_empty() {
        return Err(Error::InvalidArgument(
            "--methodN=<req>,<exp> の形式で指定してください".to_string(),
        ));
    }
    Ok(())
}

/// 使い方の表示。
pub fn help() -> String {
    include_str!("help.txt").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn device_and_methods() {
        let o = parse(&s(&[
            "-d", "/dev/sg0", "-T", "0", "-L", "-r", "a.raw", "-i", "a.iso",
        ]))
        .unwrap();
        assert_eq!(o.device.as_deref(), Some("/dev/sg0"));
        assert_eq!(o.disctype, Some(0));
        assert_eq!(o.dump_method, Some(12));
        assert_eq!(o.raw_out.as_deref(), Some(std::path::Path::new("a.raw")));
    }

    #[test]
    fn long_method_with_sec_spec() {
        let o = parse(&s(&["--method4=27,27"])).unwrap();
        assert_eq!(o.dump_method, Some(4));
        assert_eq!(o.sec_disc, Some(27));
        assert_eq!(o.sec_mem, Some(27));
    }

    #[test]
    fn attached_short_value() {
        let o = parse(&s(&["-d/dev/sg0", "-K"])).unwrap();
        assert_eq!(o.device.as_deref(), Some("/dev/sg0"));
        assert_eq!(o.dump_method, Some(11));
    }

    #[test]
    fn combined_flags() {
        let o = parse(&s(&["-gsH"])).unwrap();
        assert!(o.gui);
        assert!(o.resume);
        assert!(o.no_hashing);
    }

    #[test]
    fn start_end_and_speed() {
        let o = parse(&s(&["-d", "x", "-t", "0", "-e", "2048", "-x", "24"])).unwrap();
        assert_eq!(o.start_sector, Some(0));
        assert_eq!(o.end_sector, Some(2048));
        assert_eq!(o.speed, Some(24));
    }

    #[test]
    fn unknown_option_errors() {
        assert!(parse(&s(&["--bogus"])).is_err());
        assert!(parse(&s(&["-Z"])).is_err());
    }
}
