//! 指定範囲を読み出して ISO/RAW のハッシュを表示する（実機検証用）。
//!
//! 使い方: dump_range <device> <method-id> <start> <end> [raw_out] [iso_out]
//! Linux のみ（SG_IO）。

fn main() {
    if !cfg!(target_os = "linux") {
        eprintln!("この example は Linux 専用です（/dev/sgN が必要）");
        std::process::exit(2);
    }
    #[cfg(target_os = "linux")]
    run();
}

#[cfg(target_os = "linux")]
fn run() {
    use friidump::disc::Disc;
    use friidump::drive::dvd::DvdDrive;
    use friidump::hasher::MultiHash;
    use friidump::metadata::DiscType;
    use friidump::read::ReadMethod;
    use std::io::Write;

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!("使い方: dump_range <device> <method> <start> <end> [raw_out] [iso_out]");
        std::process::exit(2);
    }
    let device = &args[1];
    let method_id: i32 = args[2].parse().expect("method は数値");
    let start: u32 = args[3].parse().expect("start は数値");
    let end: u32 = args[4].parse().expect("end は数値");
    let raw_out = args.get(5);
    let iso_out = args.get(6);

    let method = ReadMethod::from_id(method_id).expect("method は 0-13");
    let drive = DvdDrive::open(device, -1).expect("ドライブを開けません");
    println!("drive: {} supported={}", drive.model, drive.supported);

    let mut disc = Disc::new(drive).expect("Disc 生成失敗");
    disc.disc_type = DiscType::GameCube;
    disc.sectors_no = 712_880;
    disc.set_unscrambling(true);
    disc.set_read_method(method, None, None)
        .expect("method 設定失敗");

    let mut raw_file = raw_out.map(|p| std::fs::File::create(p).expect("raw 作成失敗"));
    let mut iso_file = iso_out.map(|p| std::fs::File::create(p).expect("iso 作成失敗"));

    let mut h_iso = MultiHash::new();
    let mut h_raw = MultiHash::new();
    let mut last_block: Option<u32> = None;

    for s in start..end {
        disc.read_sector(s).expect("read_sector 失敗");
        let block = s / 16;
        if last_block != Some(block) {
            last_block = Some(block);
            let (d, r) = disc.sector(s).expect("キャッシュ参照失敗");
            if let Some(f) = raw_file.as_mut() {
                f.write_all(r).expect("raw 書き込み失敗");
            }
            if let Some(f) = iso_file.as_mut() {
                f.write_all(d).expect("iso 書き込み失敗");
            }
        }
        let (d, r) = disc.sector(s).expect("キャッシュ参照失敗");
        let oo = (s % 16) as usize * 2048;
        let ro = (s % 16) as usize * 2064;
        h_iso.update(&d[oo..oo + 2048]);
        h_raw.update(&r[ro..ro + 2064]);
        if (s - start) % 1600 == 0 {
            eprintln!("  ... sector {s}");
        }
    }

    let di = h_iso.finish();
    let dr = h_raw.finish();
    println!("ISO  MD5={} CRC32={}", di.md5, di.crc32);
    println!("RAW  MD5={} CRC32={}", dr.md5, dr.crc32);
}
