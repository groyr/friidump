//! friidump: GameCube / Wii ディスク吸い出し CLI。
//!
//! - `-u`: raw イメージを ISO に変換
//! - `-d`: ドライブから吸い出し（Linux のみ。SG_IO 経由）

use friidump::cli::{self, Options};
use friidump::error::{Error, Result};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{}", cli::help());
        return ExitCode::FAILURE;
    }
    let opts = match cli::parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("引数エラー: {e}");
            eprintln!("{}", cli::help());
            return ExitCode::FAILURE;
        }
    };
    if opts.help {
        println!("{}", cli::help());
        return ExitCode::SUCCESS;
    }

    match run(&opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("エラー: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(opts: &Options) -> Result<()> {
    if let Some(input) = &opts.raw_in {
        return run_unscramble(opts, input);
    }
    if opts.device.is_some() {
        #[cfg(target_os = "linux")]
        {
            return run_device(opts);
        }
        #[cfg(not(target_os = "linux"))]
        {
            return Err(Error::Unsupported(
                "ドライブ吸い出しは現在 Linux のみ対応です".to_string(),
            ));
        }
    }
    Err(Error::InvalidArgument(
        "操作が指定されていません（-d か -u を使ってください）".to_string(),
    ))
}

/// raw イメージを ISO に変換する（`-u`）。
fn run_unscramble(opts: &Options, input: &std::path::Path) -> Result<()> {
    use friidump::unscrambler::{Unscrambler, DISCTYPE_DVD, DISCTYPE_NINTENDO};

    let output = opts
        .iso_out
        .as_ref()
        .ok_or_else(|| Error::InvalidArgument("-u には -i <出力> が必要です".to_string()))?;

    let mut u = Unscrambler::new();
    u.set_disctype(if opts.disctype == Some(3) {
        DISCTYPE_DVD
    } else {
        DISCTYPE_NINTENDO
    });
    u.set_bruteforce(true);

    let mut state = ProgressState::new(opts.gui);
    let mut progress = move |start, done, total| state.update(start, done, total);
    u.unscramble_file(
        &input.to_string_lossy(),
        &output.to_string_lossy(),
        Some(&mut progress),
    )?;
    eprintln!("スクランブル解除が完了しました: {}", output.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_device(opts: &Options) -> Result<()> {
    use friidump::constants::{
        DISC_GAMECUBE_SECTORS_NO, DISC_WII_SECTORS_NO_DL, DISC_WII_SECTORS_NO_SL,
    };
    use friidump::disc::Disc;
    use friidump::drive::dvd::DvdDrive;
    use friidump::metadata::{self, DiscType};
    use friidump::read::ReadMethod;

    let device = opts.device.as_ref().unwrap();
    let drive = DvdDrive::open(device, opts.command)?;
    eprintln!(
        "ドライブ: {}  (対応: {})",
        drive.model,
        if drive.supported {
            "はい"
        } else {
            "いいえ"
        }
    );
    let mut disc = Disc::new(drive)?;

    let (dtype, sectors, layerbreak) = match opts.disctype {
        Some(0) => (DiscType::GameCube, DISC_GAMECUBE_SECTORS_NO, 0),
        Some(1) => (DiscType::Wii, DISC_WII_SECTORS_NO_SL, 0),
        Some(2) => {
            let lb = disc.drive.get_layerbreak().unwrap_or(0);
            (DiscType::WiiDl, DISC_WII_SECTORS_NO_DL, lb)
        }
        Some(3) => {
            let s = disc.drive.get_size().unwrap_or(0);
            let lb = disc.drive.get_layerbreak().unwrap_or(0);
            (DiscType::Dvd, s, lb)
        }
        _ => detect_type(&mut disc)?,
    };
    disc.disc_type = dtype;
    disc.sectors_no = opts.sectors_no.unwrap_or(sectors);
    disc.layerbreak = layerbreak;
    disc.set_unscrambling(true);

    let method = opts
        .dump_method
        .and_then(ReadMethod::from_id)
        .or_else(|| ReadMethod::from_id(disc.drive.def_method as i32))
        .unwrap_or(ReadMethod::M12);
    disc.set_read_method(method, opts.sec_disc, opts.sec_mem)?;

    if opts.stop_unit {
        let ok = disc.drive.stop_unit(false).is_ok();
        eprintln!("STOP コマンド: {}", if ok { "OK" } else { "失敗" });
        return Ok(());
    }
    let _ = disc.drive.stop_unit(true);

    eprintln!(
        "ディスク: type={} size={} method={}{}",
        dtype.as_str(),
        disc.sectors_no,
        method.id(),
        if disc.layerbreak > 0 {
            format!(" layerbreak={}", disc.layerbreak)
        } else {
            String::new()
        }
    );

    // メタデータ（GC/Wii のみ）
    let mut auto_name: Option<String> = None;
    if dtype != DiscType::Dvd {
        disc.read_sector(0)?;
        if let Some((data, _)) = disc.sector(0) {
            let info = metadata::parse_sector0(data)?;
            eprintln!(
                "Game ID: {}  Region: {}  Maker: {} - {}  Version: {}  Title: {}",
                info.game_id,
                info.region.as_str(),
                info.maker,
                metadata::maker_name(&info.maker),
                info.version_string,
                info.title
            );
            auto_name = Some(format!("{}.iso", info.title.trim()));
        }
    }

    if let Some(x) = opts.speed {
        disc.drive.set_speed(x * 177)?;
        disc.drive.set_streaming(x * 177)?;
    }

    let iso_out = if opts.autodump {
        auto_name.map(std::path::PathBuf::from)
    } else {
        opts.iso_out.clone()
    };
    if opts.raw_out.is_none() && iso_out.is_none() {
        eprintln!("出力ファイルが指定されていません（-r / -i / -a）");
        return Ok(());
    }

    {
        use friidump::dumper::Dumper;
        let mut dumper = Dumper::new(&mut disc);
        dumper.set_hashing(!opts.no_hashing);
        if let Some(s) = opts.start_sector {
            dumper.set_start_sector(s);
        }
        if let Some(e) = opts.end_sector {
            dumper.set_end_sector(e);
        }
        if let Some(p) = opts.raw_out.as_ref() {
            dumper.set_raw_output(p, opts.resume)?;
        }
        if let Some(p) = iso_out.as_ref() {
            dumper.set_iso_output(p, opts.resume)?;
        }
        let mut state = ProgressState::new(opts.gui);
        dumper.set_progress(Box::new(move |s, d, t| state.update(s, d, t)));
        dumper.prepare(opts.resume)?;
        dumper.dump()?;

        if !opts.no_hashing {
            if opts.raw_out.is_some() {
                if let Some(d) = dumper.raw_digests() {
                    eprintln!(
                        "RAW ハッシュ: CRC32={} MD5={} SHA-1={}",
                        d.crc32, d.md5, d.sha1
                    );
                }
            }
            if iso_out.is_some() {
                if let Some(d) = dumper.iso_digests() {
                    eprintln!(
                        "ISO ハッシュ: CRC32={} MD5={} SHA-1={}",
                        d.crc32, d.md5, d.sha1
                    );
                }
            }
        }
    }
    Ok(())
}

/// ディスク種別を自動判定する（C 版 `disc_detect_type` の簡易版）。
#[cfg(target_os = "linux")]
fn detect_type<D: friidump::drive::device::ScsiDevice>(
    disc: &mut friidump::disc::Disc<D>,
) -> Result<(friidump::metadata::DiscType, u32, u32)> {
    use friidump::constants::{
        DISC_GAMECUBE_SECTORS_NO, DISC_WII_SECTORS_NO_DL, DISC_WII_SECTORS_NO_SL,
    };
    use friidump::metadata::DiscType;

    let out = disc
        .drive
        .read_sector_dummy(DISC_GAMECUBE_SECTORS_NO + 100, 16)?;
    if out.failed && out.sense.sense_key == 0x05 && out.sense.asc == 0x21 {
        return Ok((DiscType::GameCube, DISC_GAMECUBE_SECTORS_NO, 0));
    }
    let out = disc
        .drive
        .read_sector_dummy(DISC_WII_SECTORS_NO_SL + 100, 16)?;
    if out.failed && out.sense.sense_key == 0x05 && out.sense.asc == 0x21 {
        return Ok((DiscType::Wii, DISC_WII_SECTORS_NO_SL, 0));
    }
    let lb = disc.drive.get_layerbreak().unwrap_or(0);
    Ok((DiscType::WiiDl, DISC_WII_SECTORS_NO_DL, lb))
}

/// 進捗表示の状態。
struct ProgressState {
    start: Option<std::time::Instant>,
    gui: bool,
}

impl ProgressState {
    fn new(gui: bool) -> Self {
        Self { start: None, gui }
    }

    fn update(&mut self, start: bool, done: u32, total: u32) {
        use std::io::Write;
        if start {
            self.start = Some(std::time::Instant::now());
            return;
        }
        let elapsed = self.start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
        let pct = if total > 0 {
            100.0 * done as f64 / total as f64
        } else {
            0.0
        };
        let mb = done as f64 * 2064.0 / 1024.0 / 1024.0;
        let mbs = if elapsed > 0.0 { mb / elapsed } else { 0.0 };
        if self.gui {
            println!("{pct:.0}%|{done}/{total} sectors|{mb:.2} MB|{elapsed:.0}s|{mbs:.2} MB/s");
        } else {
            print!("\r{pct:3.0}% | {done}/{total} sectors | {mb:.1} MB | {mbs:.2} MB/s   ");
            let _ = std::io::stdout().flush();
            if done >= total {
                println!();
            }
        }
    }
}
