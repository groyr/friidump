# AGENTS.md

このリポジトリは GameCube / Wii のディスクをベンダコマンドで読み出す CLI
**friidump** の Rust 実装です。元は Arep による C 実装 FriiDump（GPLv2+）で、
本リポジトリはその Rust 再実装です。

## ビルド・テスト

```sh
cargo test                # 単体テスト + ゴールデンベクタ
cargo build --release     # → target/release/friidump
```

- 読み出しは Linux の SG_IO（文字デバイス `/dev/sgN`）を使用。Windows 用バックエンドは未実装
- Raspberry Pi(armv6) ネイティブビルド可（rustup の `arm-unknown-linux-gnueabihf`）
- debug は seed 総当たりが遅いため、実機は `--release` 推奨（`cargo test` は debug で可）

## 使い方

```sh
# 吸い出し（GameCube / method12、部分範囲）
target/release/friidump -d /dev/sg0 -T 0 -L -t 0 -e 2049 -r out.raw -i out.iso
# raw → ISO 変換
target/release/friidump -u out.raw -i out.iso
```

- `-e/--stopsector` は **exclusive**（指定セクタを含まない）
- `-d` は SCSI generic 文字デバイス（`/dev/sgN`）を指定。ブロックデバイス（`/dev/srN`）では
  ベンダコマンド `0xe7` がカーネルに拒否される
- `-T <n>` で型強制（0=GameCube / 1=Wii / 2=Wii_DL / 3=DVD）
- 対応ドライブの既定 method は `src/drive/profile.rs` が決める

## リポジトリ構成

| パス | 内容 |
|---|---|
| `src/ecma267.rs` / `src/unscrambler.rs` | EDC/LFSR・seed クラック・復号 |
| `src/metadata.rs` / `src/hasher.rs` | セクタ 0 解析・CRC32/MD5/SHA-1 |
| `src/drive/{mmc,profile,device,dvd}.rs` | CDB・ドライブ特性・SG_IO・INQUIRY/READ/E7 |
| `src/cache.rs` / `src/read/mod.rs` / `src/disc.rs` | ブロックキャッシュ・方式パラメータ・method11/12 |
| `src/dumper.rs` / `src/cli.rs` / `src/main.rs` | 出力・ジャーナル・resume・CLI |
| `tests/golden.rs` + `tests/vectors/` | C 実装由来のゴールデンベクタとの突き合わせ |
| `tools/probe/` | ドライブ調査プローブ（Python、過去の調査用） |
| `docs/` | 上流ドキュメント |

## 読み出し方式

| method | 内容 | 備考 |
|---|---|---|
| 11 (`-K`) | fast 方式（READ×2 + E7 先頭6B + 位相ごとの校正で復号） | |
| 12 (`-L`) | fast + raw マスター（E7 先頭12B/末尾10B + 全ブロック EDC） | GCC-4160N/4240N の既定 |
| 0-10, 13 | 未実装（他ドライブ用） | method13 は Hitachi Type2 |

- **fast 方式の原理**: 通常 READ(12)（streaming）が返す host データは
  `raw[12:2060] XOR drive_cipher(位相)`。GC 側・ドライブ側の cipher は 16 ブロック周期。
  代表 16 ブロックを E7 で読み `corr[位相] = drive_cipher XOR gc_cipher` を校正すれば、
  以降は `rd XOR corr` で復号できる。各セクタ先頭 6B は E7 で補う
- **E7 ベースアドレスは機種依存**。GCC-4160N/4240N は `0xA13000`（`0x80000000` は誤り）
- block0 と二層の層先頭は seed 例外のため raw 経路で読む
- **二層(Wii DL)**: 第2層の物理セクタ番号オフセットを検出し、層別に校正テーブルを持つ
  （第2層は逆回転ではなく LBA をそのまま READ に渡せば読める）

## 実測（Pi Zero W + GCC-4240N + Initio 13FD:1040）

- method12 で **~0.3 MB/s**（ドライブ律速）。単一 READ は stale を返すため 2 回 READ が必要
- フル吸い出し（GameCube 712,880 セクタ）検証: Rust 版の ISO が redump と完全一致
  （CRC-32 `60aefa3e` / MD5 `20cdb87874ce4f2db4717fb43682e026` / SHA-1 `14ddb656…`）

## 既知の問題（重要）

### 1. USB ブリッジ（Initio 13FD:1040）の切断

- 長時間の読み出し中にドライブが USB バスから切断する
  （`usb 1-1: device descriptor read/64, error -110` → `USB disconnect`。復帰せず物理再挿しが必要）
- 症状: 吸い出しがゼロで埋まる。破損箇所は実行ごとに変わる
- 対策: **別の USB-IDE/SATA ブリッジ**を使う。長時間運用はチャンク分割＋再挿し前提
- journal（`<file>.journal` の `next=<sector>`）と `-s` で途中再開できる

### 2. Pi Zero W でドライブ接続時に Pi がハング → `dwc_otg.fiq_enable=0` で解消

- 症状: 接続の**瞬間に Pi がハング**（USB 列挙ログすら残らない）。watchdog(1分)で再起動するが、
  接続されたままだと再ハングする（ドライブを外して電源再投入が必要）
- 原因: Pi Zero W の `dwc_otg` とブリッジの相互作用。ユーザ空間到達前のカーネル内で停止
- **対策**: `/boot/firmware/cmdline.txt` に `dwc_otg.fiq_enable=0` を追加
  - `dwc_otg.fiq_fsm_enable=0` 単独 / `dwc_otg.speed=1` 単独では解消しない
  - 反映後は `RW/DVD GCC-4240N` が high-speed で正常列挙し `/dev/sg0`・`/dev/sr0` を生成
- カーネル/ファームは不変。ソフト回帰ではなくハード/ドライバ相互作用。同ブリッジは Windows(xHCI) では正常

### 3. ディスクの限界領域

- 一部ディスクで特定 LBA が時々ゼロ/時々データになる（傷・汚れの可能性。清掃で改善することも）

## バージョン管理

- **jj (Jujutsu)**（`jj git init --colocate` 済み。LF 運用）
- 作業前に `jj new`、完了時に `jj describe`（日本語）
- 上流へは PR しない

## 将来の課題

1. method0-10（他ドライブ用）と Type2 の method13 の実装
2. Windows SPTI バックエンド
3. 出力 I/O のスレッド分離（読み出しと書き込みの直列化を解消）
4. CI（`cargo test` とリリースビルドの常時実行）
5. Wii DL の redump 照合（良質なブリッジ入手後）

## ライセンス

GPL-2.0-or-later（`COPYING`）。帰属は `AUTHORS`・`PATCHES.md`・`docs/` を参照。
