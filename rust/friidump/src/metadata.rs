//! ディスクのメタデータ解析。
//!
//! C 実装 `libfriidump/disc.c` の `disc_analyze()`・region/maker テーブル・型名テーブルを
//! 移植する。セクタ 0 のスクランブル解除済みバッファ（2048 バイト）を解析する純粋関数で、
//! デバイス I/O には依存しない。

use crate::constants::SECTOR_SIZE;
use crate::error::{Error, Result};

/// ディスク種別（C 版 `disc_type`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscType {
    /// GameCube。
    GameCube,
    /// Wii（単層）。
    Wii,
    /// Wii（二層）。
    WiiDl,
    /// 通常 DVD。
    Dvd,
}

impl DiscType {
    /// 表示用の文字列（C 版 `disc_type_strings`）。
    pub fn as_str(self) -> &'static str {
        match self {
            DiscType::GameCube => "GameCube",
            DiscType::Wii => "Wii",
            DiscType::WiiDl => "Wii_DL",
            DiscType::Dvd => "DVD",
        }
    }

    /// Wii 系（単層・二層）かどうか。
    pub fn is_wii(self) -> bool {
        matches!(self, DiscType::Wii | DiscType::WiiDl)
    }
}

/// ディスクリージョン（C 版 `disc_region`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscRegion {
    /// ヨーロッパ/PAL。
    Pal,
    /// アメリカ/NTSC。
    Ntsc,
    /// 日本/NTSC。
    Japan,
    /// オーストラリア/PAL。
    Australia,
    /// フランス/PAL。
    France,
    /// ドイツ/PAL。
    Germany,
    /// イタリア/PAL。
    Italy,
    /// スペイン/PAL。
    Spain,
    /// ヨーロッパ(X)/PAL。
    PalX,
    /// ヨーロッパ(Y)/PAL。
    PalY,
    /// 不明。
    Unknown,
}

impl DiscRegion {
    /// 表示用の文字列（C 版 `disc_region_strings`）。
    pub fn as_str(self) -> &'static str {
        match self {
            DiscRegion::Pal => "Europe/PAL",
            DiscRegion::Ntsc => "USA/NTSC",
            DiscRegion::Japan => "Japan/NTSC",
            DiscRegion::Australia => "Australia/PAL",
            DiscRegion::France => "France/PAL",
            DiscRegion::Germany => "Germany/PAL",
            DiscRegion::Italy => "Italy/PAL",
            DiscRegion::Spain => "Spain/PAL",
            DiscRegion::PalX => "Europe(X)/PAL",
            DiscRegion::PalY => "Europe(Y)/PAL",
            DiscRegion::Unknown => "Unknown",
        }
    }

    /// リージョンコード 1 文字から変換する（C 版 `disc_analyze` の switch）。
    fn from_code(c: u8) -> Self {
        match c {
            b'P' => DiscRegion::Pal,
            b'E' => DiscRegion::Ntsc,
            b'J' => DiscRegion::Japan,
            b'U' => DiscRegion::Australia,
            b'F' => DiscRegion::France,
            b'D' => DiscRegion::Germany,
            b'I' => DiscRegion::Italy,
            b'S' => DiscRegion::Spain,
            b'X' => DiscRegion::PalX,
            b'Y' => DiscRegion::PalY,
            _ => DiscRegion::Unknown,
        }
    }
}

/// セクタ 0 から得られるディスク情報（C 版 `disc` のメタデータ部）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscInfo {
    /// ターゲット機種を示す 1 文字（'G'/'R' など）。
    pub system_id: u8,
    /// ゲーム ID（2 文字）。
    pub game_id: String,
    /// リージョン。
    pub region: DiscRegion,
    /// メーカーコード（2 文字）。
    pub maker: String,
    /// バージョン番号。
    pub version: u8,
    /// `1.xx` 形式のバージョン文字列。
    pub version_string: String,
    /// ゲームタイトル。
    pub title: String,
}

/// タイトルフィールドの開始オフセット（C 版 `disc_analyze` の `0x0020`）。
const TITLE_OFFSET: usize = 0x0020;
/// タイトルフィールドの最大長（C 版 `sizeof(tmp) - 1` = `0x03E0`）。
const TITLE_MAX: usize = 0x03E0;

/// スクランブル解除済みのセクタ 0 を解析する（C 版 `disc_analyze`）。
pub fn parse_sector0(buf: &[u8]) -> Result<DiscInfo> {
    if buf.len() < SECTOR_SIZE {
        return Err(Error::InvalidArgument(format!(
            "セクタ 0 のバッファ長が不足しています（{} < {}）",
            buf.len(),
            SECTOR_SIZE
        )));
    }

    let system_id = buf[0];
    let game_id = latin1(&buf[1..3]);
    let region = DiscRegion::from_code(buf[3]);
    let maker = latin1(&buf[4..6]);
    let version = buf[7];
    let version_string = format!("1.{version:02}");
    let title = parse_title(&buf[TITLE_OFFSET..TITLE_OFFSET + TITLE_MAX]);

    Ok(DiscInfo {
        system_id,
        game_id,
        region,
        maker,
        version,
        version_string,
        title,
    })
}

/// タイトルフィールドを C 版 `strtrimr` と同じ規則で取り出す。
///
/// C 版は NUL 終端で `strlen()` するため最初の NUL までを対象とし、
/// 末尾の空白（`isspace`）を除去する。
fn parse_title(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let mut s = &raw[..end];
    while let Some((&last, rest)) = s.split_last() {
        if last.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    latin1(s)
}

/// バイト列を Latin-1 として文字列化する（非 ASCII バイトはそのまま文字に写す）。
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// メーカーコードからメーカー名を引く（C 版 `disc_get_maker`、大小文字を区別しない）。
pub fn maker_name(maker: &str) -> &'static str {
    MAKERS
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(maker))
        .map(|(_, name)| *name)
        .unwrap_or("Unknown")
}

/// メーカーコード表（C 版 `disc.c` の `makers[]` を機械抽出）。
pub static MAKERS: &[(&str, &str)] = &[
    ("0A", "Jaleco"),
    ("0B", "Coconuts Japan"),
    ("0C", "Coconuts Japan / G.X.Media"),
    ("0D", "Micronet"),
    ("0E", "Technos"),
    ("0F", "Mebio Software"),
    ("0G", "Shouei System"),
    ("0H", "Starfish"),
    ("0J", "Mitsui Fudosan / Dentsu"),
    ("0L", "Warashi Inc."),
    ("0N", "Nowpro"),
    ("0P", "Game Village"),
    ("0Q", "IE Institute"),
    ("01", "Nintendo"),
    ("02", "Rocket Games / Ajinomoto"),
    ("03", "Imagineer-Zoom"),
    ("04", "Gray Matter"),
    ("05", "Zamuse"),
    ("06", "Falcom"),
    ("07", "Enix"),
    ("08", "Capcom"),
    ("09", "Hot B Co."),
    ("1A", "Yanoman"),
    ("1C", "Tecmo Products"),
    ("1D", "Japan Glary Business"),
    ("1E", "Forum / OpenSystem"),
    ("1F", "Virgin Games (Japan)"),
    ("1G", "SMDE"),
    ("1J", "Daikokudenki"),
    ("1P", "Creatures Inc."),
    ("1Q", "TDK Deep Impresion"),
    ("2A", "Culture Brain"),
    ("2C", "Palsoft"),
    ("2D", "Visit Co.,Ltd."),
    ("2E", "Intec"),
    ("2F", "System Sacom"),
    ("2G", "Poppo"),
    ("2H", "Ubisoft Japan"),
    ("2J", "Media Works"),
    ("2K", "NEC InterChannel"),
    ("2L", "Tam"),
    ("2M", "Jordan"),
    ("2N", "Smilesoft / Rocket"),
    ("2Q", "Mediakite"),
    ("3B", "Arcade Zone Ltd"),
    ("3C", "Entertainment International / Empire Software"),
    ("3D", "Loriciel"),
    ("3E", "Gremlin Graphics"),
    ("3F", "K.Amusement Leasing Co."),
    ("4B", "Raya Systems"),
    ("4C", "Renovation Products"),
    ("4D", "Malibu Games"),
    ("4F", "Eidos"),
    ("4G", "Playmates Interactive"),
    ("4J", "Fox Interactive"),
    ("4K", "Time Warner Interactive"),
    ("4Q", "Disney Interactive"),
    ("4S", "Black Pearl"),
    ("4U", "Advanced Productions"),
    ("4X", "GT Interactive"),
    ("4Y", "RARE"),
    ("4Z", "Crave Entertainment"),
    ("5A", "Mindscape / Red Orb Entertainment"),
    ("5B", "Romstar"),
    ("5C", "Taxan"),
    ("5D", "Midway / Tradewest"),
    ("5F", "American Softworks"),
    ("5G", "Majesco Sales Inc"),
    ("5H", "3DO"),
    ("5K", "Hasbro"),
    ("5L", "NewKidCo"),
    ("5M", "Telegames"),
    ("5N", "Metro3D"),
    ("5P", "Vatical Entertainment"),
    ("5Q", "LEGO Media"),
    ("5S", "Xicat Interactive"),
    ("5T", "Cryo Interactive"),
    ("5W", "Red Storm Entertainment"),
    ("5X", "Microids"),
    ("5Z", "Data Design / Conspiracy / Swing"),
    ("6B", "Laser Beam"),
    ("6E", "Elite Systems"),
    ("6F", "Electro Brain"),
    ("6G", "The Learning Company"),
    ("6H", "BBC"),
    ("6J", "Software 2000"),
    ("6K", "UFO Interactive Games"),
    ("6L", "BAM! Entertainment"),
    ("6M", "Studio 3"),
    ("6Q", "Classified Games"),
    ("6S", "TDK Mediactive"),
    ("6U", "DreamCatcher"),
    ("6V", "JoWood Produtions"),
    ("6W", "Sega"),
    ("6X", "Wannado Edition"),
    ("6Y", "LSP (Light & Shadow Prod.)"),
    ("6Z", "ITE Media"),
    ("7A", "Triffix Entertainment"),
    ("7C", "Microprose Software"),
    ("7D", "Sierra / Universal Interactive"),
    ("7F", "Kemco"),
    ("7G", "Rage Software"),
    ("7H", "Encore"),
    ("7J", "Zoo"),
    ("7K", "Kiddinx"),
    ("7L", "Simon & Schuster Interactive"),
    ("7M", "Asmik Ace Entertainment Inc."),
    ("7N", "Empire Interactive"),
    ("7Q", "Jester Interactive"),
    ("7S", "Rockstar Games"),
    ("7T", "Scholastic"),
    ("7U", "Ignition Entertainment"),
    ("7V", "Summitsoft"),
    ("7W", "Stadlbauer"),
    ("8B", "BulletProof Software (BPS)"),
    ("8C", "Vic Tokai Inc."),
    ("8E", "Character Soft"),
    ("8F", "I'Max"),
    ("8G", "Saurus"),
    ("8J", "General Entertainment"),
    ("8N", "Success"),
    ("8P", "Sega Japan"),
    ("9A", "Nichibutsu / Nihon Bussan"),
    ("9B", "Tecmo"),
    ("9C", "Imagineer"),
    ("9F", "Nova"),
    ("9G", "Take2 / Den'Z / Global Star"),
    ("9H", "Bottom Up"),
    ("9J", "TGL (Technical Group Laboratory)"),
    ("9L", "Hasbro Japan"),
    ("9N", "Marvelous Entertainment"),
    ("9P", "Keynet Inc."),
    ("9Q", "Hands-On Entertainment"),
    ("12", "Infocom"),
    ("13", "Electronic Arts Japan"),
    ("15", "Cobra Team"),
    ("16", "Human / Field"),
    ("17", "KOEI"),
    ("18", "Hudson Soft"),
    ("19", "S.C.P."),
    ("20", "Destination Software / Zoo Games / KSS"),
    ("21", "Sunsoft / Tokai Engineering"),
    ("22", "POW (Planning Office Wada) / VR1 Japan"),
    ("23", "Micro World"),
    ("25", "San-X"),
    ("26", "Enix"),
    ("27", "Loriciel / Electro Brain"),
    ("28", "Kemco Japan"),
    ("29", "Seta"),
    ("30", "Viacom"),
    ("31", "Carrozzeria"),
    ("32", "Dynamic"),
    ("34", "Magifact"),
    ("35", "Hect"),
    ("36", "Codemasters"),
    ("37", "Taito / GAGA Communications"),
    ("38", "Laguna"),
    ("39", "Telstar / Event / Taito"),
    ("40", "Seika Corp."),
    ("41", "Ubi Soft Entertainment"),
    ("42", "Sunsoft US"),
    ("44", "Life Fitness"),
    ("46", "System 3"),
    ("47", "Spectrum Holobyte"),
    ("49", "IREM"),
    ("50", "Absolute Entertainment"),
    ("51", "Acclaim"),
    ("52", "Activision"),
    ("53", "American Sammy"),
    ("54", "Take 2 Interactive / GameTek"),
    ("55", "Hi Tech"),
    ("56", "LJN LTD."),
    ("58", "Mattel"),
    ("60", "Titus"),
    ("61", "Virgin Interactive"),
    ("62", "Maxis"),
    ("64", "LucasArts Entertainment"),
    ("67", "Ocean"),
    ("68", "Bethesda Softworks"),
    ("69", "Electronic Arts"),
    ("70", "Atari (Infogrames)"),
    ("71", "Interplay"),
    ("72", "JVC (US)"),
    ("73", "Parker Brothers"),
    ("75", "Sales Curve (Storm / SCI)"),
    ("78", "THQ"),
    ("79", "Accolade"),
    ("80", "Misawa"),
    ("81", "Teichiku"),
    ("82", "Namco Ltd."),
    ("83", "LOZC"),
    ("84", "KOEI"),
    ("86", "Tokuma Shoten Intermedia"),
    ("87", "Tsukuda Original"),
    ("88", "DATAM-Polystar"),
    ("90", "Takara Amusement"),
    ("91", "Chun Soft"),
    ("92", "Video System / Mc O' River"),
    ("93", "BEC"),
    ("95", "Varie"),
    ("96", "Yonezawa / S'pal"),
    ("97", "Kaneko"),
    ("99", "Marvelous Entertainment"),
    ("A0", "Telenet"),
    ("A1", "Hori"),
    ("A4", "Konami"),
    ("A5", "K.Amusement Leasing Co."),
    ("A6", "Kawada"),
    ("A7", "Takara"),
    ("A9", "Technos Japan Corp."),
    ("AA", "JVC / Victor"),
    ("AC", "Toei Animation"),
    ("AD", "Toho"),
    ("AF", "Namco"),
    ("AG", "Media Rings Corporation"),
    ("AH", "J-Wing"),
    ("AJ", "Pioneer LDC"),
    ("AK", "KID"),
    ("AL", "Mediafactory"),
    ("AP", "Infogrames / Hudson"),
    ("AQ", "Kiratto. Ludic Inc"),
    ("B0", "Acclaim Japan"),
    ("B1", "ASCII"),
    ("B2", "Bandai"),
    ("B4", "Enix"),
    ("B6", "HAL Laboratory"),
    ("B7", "SNK"),
    ("B9", "Pony Canyon"),
    ("BA", "Culture Brain"),
    ("BB", "Sunsoft"),
    ("BC", "Toshiba EMI"),
    ("BD", "Sony Imagesoft"),
    ("BF", "Sammy"),
    ("BG", "Magical"),
    ("BH", "Visco"),
    ("BJ", "Compile"),
    ("BL", "MTO Inc."),
    ("BN", "Sunrise Interactive"),
    ("BP", "Global A Entertainment"),
    ("BQ", "Fuuki"),
    ("C0", "Taito"),
    ("C2", "Kemco"),
    ("C3", "Square"),
    ("C4", "Tokuma Shoten"),
    ("C5", "Data East"),
    ("C6", "Tonkin House / Tokyo Shoseki"),
    ("C8", "Koei"),
    ("CA", "Konami / Ultra / Palcom"),
    ("CB", "NTVIC / VAP"),
    ("CC", "Use Co.,Ltd."),
    ("CD", "Meldac"),
    ("CE", "Pony Canyon / FCI"),
    ("CF", "Angel / Sotsu Agency / Sunrise"),
    ("CG", "Yumedia / Aroma Co., Ltd"),
    ("CJ", "Boss"),
    ("CK", "Axela / Crea-Tech"),
    (
        "CL",
        "Sekaibunka-Sha / Sumire Kobo / Marigul Management Inc.",
    ),
    ("CM", "Konami Computer Entertainment Osaka"),
    ("CN", "NEC Interchannel"),
    ("CP", "Enterbrain"),
    ("CQ", "From Software"),
    ("D0", "Taito / Disco"),
    ("D1", "Sofel"),
    ("D2", "Quest / Bothtec"),
    ("D3", "Sigma"),
    ("D4", "Ask Kodansha"),
    ("D6", "Naxat"),
    ("D7", "Copya System"),
    ("D8", "Capcom Co., Ltd."),
    ("D9", "Banpresto"),
    ("DA", "Tomy"),
    ("DB", "LJN Japan"),
    ("DD", "NCS"),
    ("DE", "Human Entertainment"),
    ("DF", "Altron"),
    ("DG", "Jaleco"),
    ("DH", "Gaps Inc."),
    ("DN", "Elf"),
    ("DQ", "Compile Heart"),
    ("E0", "Jaleco"),
    ("E2", "Yutaka"),
    ("E3", "Varie"),
    ("E4", "T&ESoft"),
    ("E5", "Epoch"),
    ("E7", "Athena"),
    ("E8", "Asmik"),
    ("E9", "Natsume"),
    ("EA", "King Records"),
    ("EB", "Atlus"),
    ("EC", "Epic / Sony Records"),
    ("EE", "IGS (Information Global Service)"),
    ("EG", "Chatnoir"),
    ("EH", "Right Stuff"),
    ("EL", "Spike"),
    ("EM", "Konami Computer Entertainment Tokyo"),
    ("EN", "Alphadream Corporation"),
    ("EP", "Sting"),
    ("ES", "Star-Fish"),
    ("F0", "A Wave"),
    ("F1", "Motown Software"),
    ("F2", "Left Field Entertainment"),
    ("F3", "Extreme Ent. Grp."),
    ("F4", "TecMagik"),
    ("F9", "Cybersoft"),
    ("FB", "Psygnosis"),
    ("FE", "Davidson / Western Tech."),
    ("FK", "The Game Factory"),
    ("FL", "Hip Games"),
    ("FM", "Aspyr"),
    ("FP", "Mastiff"),
    ("FQ", "iQue"),
    ("FR", "Digital Tainment Pool"),
    ("FS", "XS Games / Jack Of All Games"),
    ("FT", "Daiwon"),
    ("G0", "Alpha Unit"),
    ("G1", "PCCW Japan"),
    ("G2", "Yuke's Media Creations"),
    ("G4", "KiKi Co Ltd"),
    ("G5", "Open Sesame Inc"),
    ("G6", "Sims"),
    ("G7", "Broccoli"),
    ("G8", "Avex"),
    ("G9", "D3 Publisher"),
    ("GB", "Konami Computer Entertainment Japan"),
    ("GD", "Square-Enix"),
    ("GE", "KSG"),
    ("GF", "Micott & Basara Inc."),
    ("GH", "Orbital Media"),
    ("GJ", "Detn8 Games"),
    ("GL", "Gameloft / Ubi Soft"),
    ("GM", "Gamecock Media Group"),
    ("GN", "Oxygen Games"),
    ("GT", "505 Games"),
    ("GY", "The Game Factory"),
    ("H1", "Treasure"),
    ("H2", "Aruze"),
    ("H3", "Ertain"),
    ("H4", "SNK Playmore"),
    ("HJ", "Genius Products"),
    ("HY", "Reef Entertainment"),
    ("HZ", "Nordcurrent"),
    ("IH", "Yojigen"),
    ("J9", "AQ Interactive"),
    ("JF", "Arc System Works"),
    ("JW", "Atari"),
    ("K6", "Nihon System"),
    ("KB", "NIS America"),
    ("KM", "Deep Silver"),
    ("LH", "Trend Verlag / East Entertainment"),
    ("LT", "Legacy Interactive"),
    ("MJ", "Mumbo Jumbo"),
    ("MR", "Mindscape"),
    ("MS", "Milestone / UFO Interactive"),
    ("MT", "Blast !"),
    ("N9", "Terabox"),
    ("NK", "Neko Entertainment / Diffusion / Naps team"),
    ("NP", "Nobilis"),
    ("NR", "Data Design / Destineer Studios"),
    ("PL", "Playlogic"),
    ("RM", "Rondomedia"),
    ("RS", "Warner Bros. Interactive Entertainment Inc."),
    ("RT", "RTL Games"),
    ("RW", "RealNetworks"),
    ("S5", "Southpeak Interactive"),
    ("SP", "Blade Interactive Studios"),
    ("SV", "SevenGames"),
    ("TK", "Tasuke / Works"),
    ("UG", "Metro 3D / Data Design"),
    ("VN", "Valcon Games"),
    ("VP", "Virgin Play"),
    ("WR", "Warner Bros. Interactive Entertainment Inc."),
    ("XJ", "Xseed Games"),
    ("XS", "Aksys Games"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用にセクタ 0 を組み立てる。
    fn build_sector0() -> [u8; SECTOR_SIZE] {
        let mut buf = [0u8; SECTOR_SIZE];
        buf[0] = b'G';
        buf[1] = b'S';
        buf[2] = b'N';
        buf[3] = b'J';
        buf[4] = b'0';
        buf[5] = b'1';
        buf[7] = 0;
        let title = b"NARUTO GEKITOU NINJA TAISEN 4";
        buf[TITLE_OFFSET..TITLE_OFFSET + title.len()].copy_from_slice(title);
        buf
    }

    #[test]
    fn parses_basic_fields() {
        let info = parse_sector0(&build_sector0()).unwrap();
        assert_eq!(info.system_id, b'G');
        assert_eq!(info.game_id, "SN");
        assert_eq!(info.region, DiscRegion::Japan);
        assert_eq!(info.maker, "01");
        assert_eq!(info.version, 0);
        assert_eq!(info.version_string, "1.00");
        assert_eq!(info.title, "NARUTO GEKITOU NINJA TAISEN 4");
    }

    #[test]
    fn trims_title_and_stops_at_nul() {
        let mut buf = [0u8; SECTOR_SIZE];
        buf[TITLE_OFFSET..TITLE_OFFSET + 5].copy_from_slice(b"ABC  ");
        buf[TITLE_OFFSET + 5] = 0;
        buf[TITLE_OFFSET + 6] = b'X'; // NUL の後は無視される
        assert_eq!(parse_sector0(&buf).unwrap().title, "ABC");
    }

    #[test]
    fn region_unknown_for_other_codes() {
        let mut buf = build_sector0();
        buf[3] = b'?';
        assert_eq!(parse_sector0(&buf).unwrap().region, DiscRegion::Unknown);
    }

    #[test]
    fn maker_lookup_is_case_insensitive() {
        assert_eq!(maker_name("01"), "Nintendo");
        assert_eq!(maker_name("zz"), "Unknown");
    }

    #[test]
    fn rejects_short_buffer() {
        assert!(parse_sector0(&[0u8; 10]).is_err());
    }
}
