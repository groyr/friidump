# FriiDump フォーク 改修点（PATCHES）

このリポジトリは [bradenmcd/friidump](https://github.com/bradenmcd/friidump) をフォークし、
**HL-DT-ST GCC-4160N / GCC-4240N（MN103S）** への対応、Linux(armv6) / Windows(MinGW) ビルド、
および読み出し高速化の調査結果を追加したものです。原典は Arep による FriiDump（GPLv2）です。

上流へ追従する際は、以下の差分を再適用してください。

## 変更一覧

1. **`libfriidump/unscrambler.h` / `unscrambler.c` — 多重定義の修正**
   - `u_int8_t disctype;` を `extern` 宣言に変更し、実体定義を `unscrambler.c` に移動
   - 背景: 最近の GCC は `-fno-common` が既定のため、ヘッダの定義が複数翻訳単位で衝突する

2. **`libfriidump/misc.c` — 暗黙宣言の修正**
   - コメントアウトされていた `#include <sys/time.h>` を有効化（`gettimeofday` の暗黙宣言を解消）

3. **`CMakeLists.txt` — ビルド互換**
   - `cmake_minimum_required(VERSION 2.8)` → `3.5`
   - 現代の GCC（C23 既定）で K&R 宣言を許容するため `-std=gnu11` を既定 C フラグに追加
   - MinGW ビルドは `-DBUILD_STATIC_BINARY=ON` を推奨

4. **`libfriidump/dvd_drive.c` — 対象ドライブの追加**
   - `dvd_is_hitachi_family()` を追加し、`HL-DT-ST` かつ製品IDに
     `GDR8082N / GDR8161B / GDR8162B / GDR8163B / GDR8164B / GCC-4160N / GCC-4240N /
     GCC-4241N / GCC-4243N / GCC-4244N / GCC-4247N` を含む場合に Hitachi MN103S（`0xe7`）方式として扱う
   - 背景: 従来は `strcmp` の完全一致で一部 GDR のみ対応しており、ENQUIRY の製品IDが
     `RW/DVD GCC-4240N` のように接頭辞付きになる機種で取りこぼしていた

5. **`libfriidump/dvd_drive.c` — Linux の SG_IO 対応**
   - Linux の `dvd_execute_cmd` を **SG_IO**（`/dev/srN`・`/dev/sgN`）優先に変更し、
     USBブリッジ越しでもベンダコマンド（`0xe7`）が転送されやすくした
   - SG_IO が使えない環境は従来の `CDROM_SEND_PACKET` にフォールバック

6. **GCC-4160N / GCC-4240N の `0xe7` ベースアドレス修正と `method10`**
   - **根因**: 従来の `HITACHI_MEM_BASE = 0x80000000` は GCC-4240N では誤り。
     データフレームは **`0xA13000`** にキャッシュされる（DIC の Type1 相当）
   - `libfriidump/hitachi.c`: `hitachi_mn103s_dump_mem()`（base `0xA13000`）を追加
   - `libfriidump/dvd_drive.c`: `dvd_assign_functions()` で GCC-4160N/4240N を
     `hitachi_mn103s_dump_mem` + `def_method = 10` に割当
   - `libfriidump/disc.c`: `disc_read_sector_10()` を追加
     （streaming READ で 16 セクタをキャッシュ → `0xA13000` から E7 で 1 ブロック取得 → セクタ番号検証 → 逆スクランブル）
   - 効果: GCC-4240N 実機で正しいデータを取得可能（従来は誤アドレスで動作不能）

7. **`libfriidump/disc.c` — method7 の E7 読み出しバッチ化**
   - 5 ブロック分のキャッシュをメモリ連続領域として、`65535` バイト以下の E7 に束ねて読む（5 回 → 3 回）

8. **`libfriidump/win32compat.h` / `win32compat.c` — MinGW 対応**
   - MinGW が `strndup` / `ftruncate` / `gettimeofday` を提供するため、
     これらを MSVC のときだけ自前宣言／定義するようガード
   - MinGW では `<unistd.h>` / `<sys/time.h>` を取り込み

9. **`tools/probe/` — 調査用プローブの同梱**
   - `0xe7` の応答時間、キャッシュ番地、スクランブル seed、fast方式の検証用 Python(SPTI) スクリプト
   - 詳細は `tools/probe/README.md`（実行時は DIC 等ドライブ使用ツールを停止すること）

## 既知の制限・注意

- **`GCC-4240N(E112)` + `Initio 13FD:1040`** は `0xe7` も通常READも通るが、
  連続読み出しで **キャッシュが 1 ブロック遅れる（デシンク）**。
  - 対策: **READ を 2 回発行してから E7 を読む**（初回 READ は吸収される。単発ではほぼ失敗）
  - 安定運用には別チップの USB ブリッジ（NEC / JMicron）を推奨
- E7 のベースアドレスは機種依存。GCC-4160N/4240N は `0xA13000`（`0x80000000` は誤り）
- スクランブル seed は **16 ブロック周期**。GC seed は disc ごとに異なるため校正が必要（block0 は例外）
- 通常 READ が返す 2048B は「`raw[12:2060] XOR 固定seed`」であり、各セクタ先頭 6B (`raw[6:12]`) を含まない
  - そのため高速化には先頭 6B を **E7** で補う必要がある（`tools/probe/` 参照）

## 高速化の調査結果（実験的・未実装）

- `0xe7` は **転送律速で上限 ~682 KB/s**（サイズ比例）。GC 全面 1,437MiB で下限 ~36 分
- fast方式: `streaming READ×2 + E7(先頭12B)×16 + 2種の16周期seed復号` で
  **112/112 ブロック完全一致**を確認（推定 GC ~45〜60 分）。friidump 本体への実装は今後

## ビルド

```bash
# Linux / Raspberry Pi (armv6)
cd friidump && mkdir -p build && cd build
cmake .. -DCMAKE_BUILD_TYPE=Release && make -j1

# Windows (MSYS2 UCRT64 + MinGW)
cmake -G "MinGW Makefiles" -DCMAKE_BUILD_TYPE=Release -DBUILD_STATIC_BINARY=ON -B build-win
cmake --build build-win -j
```

## ライセンス・帰属

- GPLv2（`COPYING` を継承）。各ソースの著作権ヘッダは保持
- 原典: Arep による FriiDump、フォーク元: bradenmcd/friidump
- 本フォークの作者は `AUTHORS` を参照
- README の免責・DMCA に関する注意も継承
