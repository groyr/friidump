# AGENTS.md（friidump フォーク）

このリポジトリは上流 [`bradenmcd/friidump`](https://github.com/bradenmcd/friidump)
（元: Arep, GPLv2+）のフォーク **`groyr/friidump`** です。
GameCube / Wii のディスクをベンダコマンドで読み出す CLI で、本フォークでは
**GCC-4160N/4240N 等の fast 読み出し（method10-12）** と **Wii DL（二層）対応** を追加しています。

## 役割分担（関連リポジトリ）

| リポジトリ | 役割 |
|---|---|
| `groyr/friidump`（本リポジトリ） | 吸い出し CLI 本体（このフォーク） |
| `groyr/cleanrip` | GC 側の吸い出し（`disc_source_net` 等）。`source/disc_scramble.c` は本家の unscrambler 由来 |
| `groyr/gc-ripper` | Pi 上で本 CLI を呼ぶ吸い出しパイプライン（RVZ→R2→通知） |
| `groyr/rvz-core` | RVZ 圧縮（Rust） |

## リポジトリ構成

- `libfriidump/` … ディスク読み出し本体
  - `dvd_drive.c` … SG_IO(Windows SPTI) で CDB を発行（`dvd_read_sector_streaming` 等）
  - `disc.c` … 型判定・seed クラック・読み出しメソッドのディスパッチ
  - `disc_fast.c` … **method11/12**（GCC-4160N/4240N の fast 読み出し。16 ブロック周期の校正）
  - `disc_hitachi.c` … method10/13
  - `drive_profile.c` … ドライブ別の 0xe7 ベースアドレス・既定メソッド
  - `dumper.c` … 出力（ISO/raw）・ジャーナル（resume）・進捗
  - `unscrambler.c` … GC スクランブル解除（16 セクタ単位）
- `libmultihash/` … MD5/SHA1/CRC32 等
- `src/` … CLI（`friidump.c`）・`getopt` 互換
- `tools/probe/` … ドライブ調査プローブ（Python、正は本リポジトリ）
- `docs/` … 上流ドキュメント（`ChangeLog` `TODO` 等）

## ビルド

### Linux（Raspberry Pi Zero / armv6）

```sh
mkdir -p build && cd build
cmake .. -DCMAKE_BUILD_TYPE=Release
make -j1
# → build/src/friidump
```

### Windows（MinGW / UCRT64）

```sh
cmake -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_STATIC_BINARY=ON \
      -DHAVE_STDBOOL_H=1 -DHAVE_FSEEKO=1 -DCMAKE_WORDS_BIGENDIAN=0 -B build-win
cmake --build build-win
```

## 読み出しメソッド

| method | 内容 | 備考 |
|---|---|---|
| 0 | 通常 READ（非ストリーミング）＋ memdump | EDC 検証あり |
| 2 | 同上（別手順） | EDC 検証あり |
| **11 (`-K`)** | fast 方式（READ×2 + E7 head + 校正で復号） | **DL の既定。唯一実用的な速度** |
| **12 (`-L`)** | fast + raw マスター（E7 head/tail + 全ブロック EDC） | GC/SL の既定。DL 第2層は極端に遅い |
| 13 (`-M`) | Type2 用スタブ（未実装） | GCC-4241N/4242N 等 |

- 既定メソッドは `drive_profile.c` が決める（GCC-4160N/4240N は method12）
- `-T <n>` で型強制（0=GC / 1=Wii / 2=Wii_DL / 3=DVD）

## Wii DL（二層）対応（本フォークで追加）

上流は型判定とサイズ定義のみで、**実際の第2層読み出しは未対応**だった。
本フォークで以下を実装:

- `dvd_get_layerbreak` を Wii_DL でも有効化（`0xAD READ DVD STRUCTURE`）
- **第2層の物理セクタ番号オフセット**を検出（実測: 第1層は `sn = LBA + 0x30000`、
  第2層は別オフセット `layer_sn_offset2`）。第2層は逆回転ではなく、LBA をそのまま READ に渡せば読める
- **層別の校正テーブル**（method11: `fast_corr2` / method12: `drive_cipher12_2`）
- 層の先頭ブロックは seed 例外のため raw 経路で読む
- **`-t/--startsector`・`-e/--stopsector`**（部分吸い出し。層境界の検証・チャンク吸い出し用。
  `-t` 指定時は出力を先頭から書く）

### 実測（Pi Zero + GCC-4240N + Initio 13FD:1040）

- ディスク: 大乱闘スマッシュブラザーズX（`RSBJ01` / Wii DL）
- layerbreak = 2,084,960 / 総セクタ = 4,155,840（8,511,160,320 B）
- 層境界（±32 セクタ）をまたぐ読み出しは **warn=0 / EDC 通過**
- 実データ速度: 第1層 ~0.3MB/s、第2層 method11 は校正後 ~0.44MB/s
- **method12 の第2層は 0.0002MB/s**（実用不可）、method0/2 は 0.017MB/s（実用不可）

## Rust 移植（`rust/friidump`、進行中）

上流 C 実装を増分移植した Rust 版。GCC-4240N の method11/12 について、Pi 実機で
C 版と **bit-exact**（ISO/RAW の MD5・CRC32 一致）を確認済み。

### ビルド・テスト

```sh
cd rust/friidump
cargo test                # 単体 45 + ゴールデン 4
cargo build --release     # → target/release/friidump-rs
```

- Pi(armv6) ネイティブビルド可（rustup の `arm-unknown-linux-gnueabihf`）
- debug は総当たりが遅いため、実機は `--release` 推奨（`cargo test` は debug で可）

### 使い方（C 版互換）

```sh
# 吸い出し（method12、部分範囲）
target/release/friidump-rs -d /dev/sg0 -T 0 -L -t 0 -e 2049 -r out.raw -i out.iso
# raw → ISO 変換
target/release/friidump-rs -u out.raw -i out.iso
```

- `-e` は **exclusive**（C 版は inclusive。help 表記に合わせて是正、リファクタ R5）
- 実機速度は C 版の約 2 倍（同一範囲 4097 セクタで C 85.5s / Rust 40.2s、user 時間 57s / 13s）
- **フル吸い出し検証（ナルト4 / GC / 712,880 セクタ）**: Rust 版の ISO が redump と
  **CRC-32 `60aefa3e` / MD5 `20cdb87874ce4f2db4717fb43682e026` / SHA-1 `14ddb656…` で完全一致**

### 構成

| モジュール | 内容 |
|---|---|
| `ecma267` / `unscrambler` | EDC/LFSR・seed クラック・復号 |
| `metadata` / `hasher` | セクタ 0 解析・CRC32/MD5/SHA-1 |
| `drive/{mmc,profile,device,dvd}` | CDB・ドライブ特性・SG_IO・INQUIRY/READ/E7 |
| `cache` / `read` / `disc` | ブロックキャッシュ・方式パラメータ・method11/12 |
| `dumper` / `cli` / `main` | 出力・ジャーナル・resume・CLI |
| `tests/golden.rs` + `tools/gen_vectors.c` | C 由来ゴールデンベクタ |

未実装: method0-10（他ドライブ用）/ Windows SPTI / `-A`。

## 既知の問題（重要）

### 1. USB ブリッジ（Initio 13FD:1040）の切断 — 最優先の課題

- 長時間の読み出し中にドライブが **USB バスから切断**する:
  `usb 1-1: device descriptor read/64, error -110` → `USB disconnect` →
  `usb1-port1: attempt power cycle`（**復帰せず、物理再挿しが必要**）
- 症状: 吸い出しが**ゼロで埋まる**（ドライブがゼロを返す）。破損箇所は実行ごとに変わる
- 外部 5V 給電でも発生。**ブリッジ自体（信号品質/ファーム）が原因**の可能性が高い
- 対策: **別の USB-IDE/SATA ブリッジ**を使う（`AGENTS.md`（cleanrip 側）にも記載）。
  長時間運用では**チャンク分割＋再挿し前提**が必要

### 1b. Pi Zero W でドライブ接続時に Pi がハング → `dwc_otg.fiq_enable=0` で解消（2026-09-24）

- 症状: ドライブを接続した**瞬間に Pi がハング**（USB 列挙ログすら残らない）。
  RPi OS 既定のハードウェア watchdog（1分）で再起動するが、再起動時にドライブが
  接続されたままだと再ハングし復帰しない（ドライブを外して電源再投入が必要）
- 原因: Pi Zero W の `dwc_otg` コントローラと Initio INIC-1511L ブリッジの
  相互作用。**ユーザ空間に到達する前**（カーネル内・USB 列挙段階）で停止する
- **対策（有効・実機確認済み）**: `/boot/firmware/cmdline.txt` に
  **`dwc_otg.fiq_enable=0`** を追加
  - `dwc_otg.fiq_fsm_enable=0` だけ / `dwc_otg.speed=1` だけでは解消しない
  - 反映後は `usb 1-1: new high-speed USB device ... Product: RW/DVD GCC-4240N` で正常列挙し、
    `/dev/sg0`・`/dev/sr0` が生成される
  - 速度影響は無視できる（吸い出し律速 ~0.44MB/s に対し USB 帯域は十分）
- 併せて **journald 永続化**（`/var/log/journal` + `SyncIntervalSec=1s`）でクラッシュログを採取可能に
- 注: カーネル/ファームは 2026-09-15 から不変。ソフト回帰ではなく
  ハード/ドライバ相互作用の顕在化。同ブリッジは Windows(xHCI) では正常動作

### 2. ディスクの限界領域

- Smash X の一部領域（LBA 88,672〜約126,000）が**時々ゼロ/時々データ**で不安定
- 傷・汚れの可能性。清掃で改善することがある

### 3. その他

- ジャーナルの `fsync` は 8192 セクタ（≒16MB）ごと。クラッシュ時は最大 ~16MB 巻き戻る
- `-t`/`-e` は今回追加した機能（上流には無い）
- 上流 `docs/TODO` の項目（big-endian のハッシュ等）は未解決のまま

## バージョン管理

- **jj (Jujutsu)**（`jj git init --colocate` 済み。LF 運用）
- ブックマーク `master`、push 先 `groyr`（= `groyr/friidump`）。上流へは PR しない
- 作業前に `jj new`、完了時に `jj describe`（日本語）

## 将来の課題

1. **USB ブリッジの置き換え**（DL 吸い出しの前提。本フォーク最大の課題）
2. **出力 I/O のスレッド分離**: `dumper.c` は読み出しと書き込みが同一ループで直列。
   別スレッド化（pthread）でドライブ律速を改善できる余地
3. **Rust 移植（`rust/friidump`）— 主要部は完了**
   - GCC-4240N の method11/12・吸い出し・`-u`・resume は実機で C と bit-exact
   - Pi(armv6) ネイティブで動作し、実機速度は C 版の約 2 倍
   - 残り: method0-10（他ドライブ用）・Windows SPTI・`-A`・CI 化
4. **CI テスト**: `cargo test`（unscrambler/EDC/seed/ゴールデンベクタ）を CI で常時実行
5. **警告強化**: `-Wextra`（必要なら `-Werror`）、`-flto` の検討
6. **`0xE7` のドライブ別対応**: GDR-8082N 等の Type2（`0x80000000` 系）実機確定
7. **Wii DL の redump 照合**: 良質なブリッジ入手後に bit-exact を検証

## ライセンス

GPL-2.0-or-later（`COPYING`）。`libmultihash` / `unscrambler` 等の帰属は
`docs/`・`PATCHES.md` を参照。
