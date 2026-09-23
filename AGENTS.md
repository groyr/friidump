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

## 既知の問題（重要）

### 1. USB ブリッジ（Initio 13FD:1040）の切断 — 最優先の課題

- 長時間の読み出し中にドライブが **USB バスから切断**する:
  `usb 1-1: device descriptor read/64, error -110` → `USB disconnect` →
  `usb1-port1: attempt power cycle`（**復帰せず、物理再挿しが必要**）
- 症状: 吸い出しが**ゼロで埋まる**（ドライブがゼロを返す）。破損箇所は実行ごとに変わる
- 外部 5V 給電でも発生。**ブリッジ自体（信号品質/ファーム）が原因**の可能性が高い
- 対策: **別の USB-IDE/SATA ブリッジ**を使う（`AGENTS.md`（cleanrip 側）にも記載）。
  長時間運用では**チャンク分割＋再挿し前提**が必要

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
3. **全面 Rust 書き換え（検討中・時期未定）**
   - 動機: メモリ安全（バッファ境界/EDC オフセット）、`Result` によるエラー処理、
     `#[test]` で unscrambler/EDC/seed を CI 常時実行、`sg3`/`nix` による型付き SG_IO
   - 留意: **吸い出しはドライブ律速で性能利得はほぼ無い**。最大コストは
     **実機で確立した method10-13・校正・0xe7 知見の再検証**。
     着手するなら「吸い出しは C のまま、RVZ/ホスト側を Rust」等の部分適用が現実的
4. **CI テスト**: unscrambler/EDC/CRC32 のホスト単体テストを CI で常時実行
5. **警告強化**: `-Wextra`（必要なら `-Werror`）、`-flto` の検討
6. **`0xE7` のドライブ別対応**: GDR-8082N 等の Type2（`0x80000000` 系）実機確定
7. **Wii DL の redump 照合**: 良質なブリッジ入手後に bit-exact を検証

## ライセンス

GPL-2.0-or-later（`COPYING`）。`libmultihash` / `unscrambler` 等の帰属は
`docs/`・`PATCHES.md` を参照。
