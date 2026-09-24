# PATCHES.md — 改修点・技術メモ

このリポジトリは、GameCube / Wii のディスクをベンダコマンドで読み出す CLI
**friidump** の **Rust 実装**です。元は Arep による C 実装 FriiDump（GPLv2+）で、
本リポジトリはその Rust 再実装です。上流 C 実装の変更履歴は git 履歴を参照。

## 対応ドライブと読み出し方式

- **HL-DT-ST GCC-4160N / GCC-4240N（Hitachi MN103S）** を対象。
  `0xe7` のメモリベースは **`0xA13000`**（従来の `0x80000000` は誤り）。
  既定 method は **12**。
- method11（fast 方式）/ method12（fast + raw + 全ブロック EDC）を実装。
- method0-10 は他ドライブ用で未実装。method13（Hitachi Type2、`0x80000000` 回転ベース・
  4 セクタ E7）も未実装スタブ。
- 機種判定・既定 method は `src/drive/profile.rs` のテーブルで管理。

## fast 方式の原理（method11/12）

通常 READ(12)（streaming）が返すホストデータ `rd` は

```
rd = ISO[6:2048] XOR gc_cipher(位相) XOR drive_cipher(位相)
```

であり、GC 側・ドライブ側の cipher はともに **16 ブロック周期**（GC seed は disc ごとに
異なるため校正が必要、block0 のみ例外）。代表 16 ブロックを E7 で生フレームごと読み、

```
corr[位相] = drive_cipher XOR gc_cipher
```

を校正しておけば、以降は

```
ISO[6:2048] = rd XOR corr[位相]
```

で復号できる。各セクタ先頭 6B（`raw[6:12]`）は通常 READ に含まれないため E7 で補う。

- **method11**: E7 で先頭 6B を取得して復号（ISO のみ）。
- **method12**: E7 で各セクタの先頭 12B・末尾 10B を取得し、`raw[12:2060] = rd XOR cipher`
  で生フレームを復元。生フレームを unscrambler に渡して **全ブロック EDC 検証**する。
  DIC/redump 互換の生 raw（真スクランブル像）を保存できる。
- 二層(Wii DL): 第2層の物理セクタ番号オフセットを検出し、層別に校正テーブルを持つ
  （第2層は LBA をそのまま READ に渡せば読める）。

## Rust 実装

| モジュール | 内容 |
|---|---|
| `src/ecma267.rs` / `src/unscrambler.rs` | EDC/LFSR・seed クラック・復号 |
| `src/metadata.rs` / `src/hasher.rs` | セクタ 0 解析・CRC32/MD5/SHA-1 |
| `src/drive/{mmc,profile,device,dvd}.rs` | CDB・ドライブ特性・SG_IO・INQUIRY/READ/E7 |
| `src/cache.rs` / `src/read/mod.rs` / `src/disc.rs` | ブロックキャッシュ・方式パラメータ・method11/12 |
| `src/dumper.rs` / `src/cli.rs` / `src/main.rs` | 出力・ジャーナル・resume・CLI |
| `tests/golden.rs` + `tests/vectors/` | C 実装由来のゴールデンベクタとの突き合わせ |

- 失敗は `Result` で伝播し、状態は構造体が所有する（グローバル可変状態なし）。
- 出力はジャーナル（`<file>.journal` の `next=<sector>`）と `-s` で resume 可能。
- `-e/--stopsector` は exclusive（指定セクタを含まない）。

### 検証（Pi Zero W + GCC-4240N + Initio 13FD:1040）

- method11/12 の ISO/RAW、CLI 吸い出し、`-u`、resume がすべて **bit-exact**
- **フル吸い出し（ナルト4 / GC / 712,880 セクタ）**: ISO が redump と完全一致
  （CRC-32 `60aefa3e` / MD5 `20cdb87874ce4f2db4717fb43682e026` / SHA-1 `14ddb656…`）
- 速度は同一範囲で C 実装の約 2 倍（4097 セクタで C 85.5s / Rust 40.2s）

## 既知の制限・注意

- **USB ブリッジ（Initio 13FD:1040）は長時間の読み出しで USB から切断する**
  - 症状: `usb 1-1: device descriptor read/64, error -110` → `USB disconnect`（復帰せず物理再挿しが必要）
  - 吸い出し結果が**ゼロで埋まる**。破損位置は実行ごとに変わる
  - 対策: **別の USB-IDE/SATA ブリッジ**を推奨。長時間運用はチャンク分割＋再挿し前提
- **Pi Zero W でドライブ接続時に Pi がハング → `dwc_otg.fiq_enable=0` で解消**
  - 接続の瞬間にハング（USB 列挙ログすら出ない）。watchdog(1分)で再起動するが、
    接続されたままだと再ハングする
  - `/boot/firmware/cmdline.txt` に `dwc_otg.fiq_enable=0` を追加
    （`fiq_fsm_enable=0` 単独 / `speed=1` 単独では解消しない）
  - カーネル/ファームは不変。ソフト回帰ではなくハード/ドライバ相互作用。
    同ブリッジは Windows(xHCI) では正常動作
- **Linux の SG_IO**: ベンダコマンド `0xe7` はブロックデバイス `/dev/srN` では `EPERM`
  （CAP_SYS_RAWIO 必須）。**文字デバイス `/dev/sgN` を `O_RDWR` で開く**こと
- GC はドライブの先読みキャッシュが stale になるため、**READ を 2 回発行**してから E7 を読む
  （method11/12 の前提）。単発 READ は無視される
- 一部ディスクで特定 LBA が時々ゼロ/時々データになる（傷・汚れの可能性）

## ビルド

```bash
# ネイティブ（Linux / Raspberry Pi armv6）
cargo build --release   # → target/release/friidump

# クロス（Windows 用など）
cargo build --release --target x86_64-pc-windows-gnu
```

- 読み出しは Linux の SG_IO を使用。Windows 用バックエンドは未実装
- 実機は `--release` 推奨（debug は seed 総当たりが遅い）

## ライセンス・帰属

- GPL-2.0-or-later（`COPYING`）。各ソースの著作権ヘッダは保持
- 原典: Arep による FriiDump。帰属は `AUTHORS`・`docs/` を参照
- README の免責・DMCA に関する注意も継承
