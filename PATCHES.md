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
   - **ブロックデバイス `/dev/srN` では `0xe7` が `EPERM`**（カーネルのコマンド許可リスト。
     CAP_SYS_RAWIO 必須）。**`O_RDWR` で開いた文字デバイス `/dev/sgN`** を使う
     （一般ユーザー/cdrom グループでも可）。そのため POSIX の open を
     `O_RDONLY` → `O_RDWR` に修正（Windows 実装も RW で開いている）

6. **GCC-4160N / GCC-4240N の `0xe7` ベースアドレス修正と `method10`**
   - **根因**: 従来の `HITACHI_MEM_BASE = 0x80000000` は GCC-4240N では誤り。
     データフレームは **`0xA13000`** にキャッシュされる（DIC の Type1 相当）
   - `libfriidump/hitachi.c`: `hitachi_mn103s_dump_mem()`（base `0xA13000`）を追加
   - `libfriidump/dvd_drive.c`: `dvd_assign_functions()` で GCC-4160N/4240N を
     `hitachi_mn103s_dump_mem` + `def_method = 10` に割当
   - `libfriidump/disc.c`: `disc_read_sector_10()` を追加
     （streaming READ で 16 セクタをキャッシュ → `0xA13000` から E7 で 1 ブロック取得 → セクタ番号検証 → 逆スクランブル）
   - 効果: GCC-4240N 実機で正しいデータを取得可能（従来は誤アドレスで動作不能）

6b. **GCC-4160N / GCC-4240N の fast方式（`method11`）**
   - 通常 `READ(12)`（streaming ビット）は `raw[12:2060] XOR cipher(drive_seed)` を返す。
     `drive_seed`・GC側 `gc_seed` はいずれも **16 ブロック周期**（GC 側は block0 のみ例外）
   - 代表 16 ブロック(16..31)を raw で読み、位相ごとの補正
     `corr[m][i] = cipher(drive_seed_m)[i] XOR cipher(gc_seed_m)[i]` を校正
   - 本体 `disc_read_sector_11()`: `streaming READ×2`（`READ` は 2 回必要）→
     `E7 12B×16`（セクタ番号検証＋先頭 6B）→ `rd[0:2042] XOR corr[m]` で復号
   - block0 は例外のため raw 経路。検証 NG は raw にフォールバック
   - 実機検証: **60000 セクタ(120MB) が DIC iso と MD5 一致**
   - `--method10` / `--method11` で明示選択可能
   - **PREFETCH(0x34) は本ドライブで ILLEGAL REQUEST のため未使用**（常に READ×2）

6c. **GCC-4160N / GCC-4240N の fast方式 + raw生成 + 全ブロックEDC（`method12`、既定）**
   - `method11` と同じく host データ(rd)から fast 復号するが、加えて
     `E7` で各セクタの生フレーム先頭 12B(`raw[0:12]`)と末尾 10B(`raw[2054:2064]`)を取得し、
     `raw[12:2060] = rd XOR drive_cipher12[位相]` で復元して生フレームを再構成する
   - `unscrambler_unscramble_16sectors` により **毎ブロック EDC 検証**。NG はリトライ→raw フォールバック
   - **raw は DIC/redump 互換の真スクランブル像**で保存
     （`disc_cache_add_block_raw`。`disc_fast_read_raw_block` は unscramble 前に退避）
   - 校正: 代表 16 ブロック(16..31)から `drive_cipher12[m][i] = rd[i] XOR raw[12+i]` を作成（block0 は例外→raw）
   - `--method12` で明示選択可能。GCC-4160N/4240N の既定
   - **実機検証（全ディスク 712880 セクタ）**:
     - ISO MD5 = `60a52be1d4d5fadc3838d73d6de200f5`（redump 一致）
     - RAW MD5 = `78c7710292d9709f7c3f03375f2a0a72`（DIC `gc2.raw` 一致）
     - ~0.32 MB/s（GC 換算 ~62分。method11 と同等速度で raw+EDC を追加）
   - **Pi(armv6) 実機の全ディスク検証（2026-09-20, OTG接続）**:
     - ISO MD5 = redump `60a52be1d4d5fadc3838d73d6de200f5`（1,459,978,240B）
     - `/dev/sgN` + `O_RDWR` で ~0.31 MB/s（~80分）

6d. **`libfriidump/dumper.c` — ジャーナル（テール破損対策 / resume 安全化）と出力 I/O の緩和**
   - `<raw|iso>.journal` に **EDC検証済みの完了位置**を **8192 セクタ毎**（≒16MB）に `fsync` 記録
     （当初は 320 セクタ毎。fsync が読み出しループを止め SD の書き込みスパイクを招くため緩和）
   - 出力(raw/iso)に **1MB の stdio バッファ**を設定し、**毎セクタ `fflush` を撤去**
     （進捗更新・ジャーナルと同じ 320 セクタ粒度でまとめて flush）
   - resume 時はファイルサイズとジャーナルの**小さい方**を採用し、未フラッシュ／破損テールを切り捨て
   - 破壊テスト（末尾破壊 → ジャーナル手前から再開）で ISO/RAW の一致回復を確認

6e. **`src/friidump.c` — 終了コードの修正**
   - `main` が成功時に `out`(=1) を返していた（失敗時に 0）。`ret`（`EXIT_SUCCESS`/`EXIT_FAILURE`）を返すよう修正
   - 効果: 呼び出し側（GC Ripper サーバ等）が exit コードで成否判定できる

7. **`libfriidump/disc.c` — method7 の E7 読み出しバッチ化**
   - 5 ブロック分のキャッシュをメモリ連続領域として、`65535` バイト以下の E7 に束ねて読む（5 回 → 3 回）

8. **`libfriidump/win32compat.h` / `win32compat.c` — MinGW 対応**
   - MinGW が `strndup` / `ftruncate` / `gettimeofday` を提供するため、
     これらを MSVC のときだけ自前宣言／定義するようガード
   - MinGW では `<unistd.h>` / `<sys/time.h>` を取り込み

9. **`tools/probe/` — 調査用プローブの同梱**
   - `0xe7` の応答時間、キャッシュ番地、スクランブル seed、fast方式の検証用 Python(SPTI) スクリプト
   - 追加: USB接続速度（`usb_speed.py`）、READ/E7 レイテンシとブロック内訳（`bench.py` / `probe_timing.py`）、
     キャッシュ窓・整列（`probe_cache.py` / `probe_alignment.py`）、速度/モード掃引（`probe_speed_modes.py`）、
     窓移動（`probe_invalidate.py`）、非同期重畳（`probe_async.py`）、CAV・総コスト（`probe_profile.py` / `probe_total.py`）、
     通常DVDでの切り分け（`probe_normal.py` / `probe_normal2.py`）、GCのREADサイズ・単発READ挙動
     （`probe_gc_size.py` / `probe_gc_seq.py` / `probe_gc_force.py`）、メディア取りこぼし切り分け（`probe_drop.py`）
   - 詳細は `tools/probe/README.md`（実行時は DIC 等ドライブ使用ツールを停止すること）

10. **`libfriidump/` — 構造整理（データ駆動化と読み出し方式の分離）**
    - `drive_profile.{c,h}` を追加し、機種ごとの特性（memdump / E7 ベース / 読み出しファミリ /
      既定 method / コマンド）をテーブル化。`dvd_assign_functions()` はテーブル参照に変更
    - `disc_fast.c` に method11/12（fast方式・校正・EDC）、`disc_hitachi.c` に method7-10 を移設
    - `disc_internal.h` に disc 構造体と共有ヘルパーを集約（実装ファイル間のみで共有）
    - `disc.c` はディスパッチと汎用 method0-6 のみに
    - **挙動は不変**（GCC-4240N は従来どおり method12 を選択）。armv6 ビルド確認済み

11. **`disc_hitachi.c` / `dvd_drive.c` / `src/friidump.c` — Type2（GCC-4241N/4242N）の下地**
    - drive_profile に `READ_FAMILY_HITACHI_TYPE2`（`0x80000000` 回転ベース・4セクタE7）を追加し、
      `GCC-4241N` / `GCC-4242N` を method13 に割当（4242N は新規認識）
    - `disc_read_sector_13()` は **未実装スタブ**（誤ったデータを返さず明示的に失敗）
    - `--method13` と help を追加

12. **`libfriidump/` — Wii DL（二層）の第2層読み出しに対応**
    - `disc.c`: Wii_DL 判定時に `dvd_get_layerbreak` を有効化（`0xAD READ DVD STRUCTURE`）
    - `disc_fast.c`: 第2層の物理セクタ番号オフセット `layer_sn_offset2` を検出し、層別に
      校正テーブルを持つ（method11: `fast_corr2` / method12: `drive_cipher12_2`）。
      第2層は逆回転ではなく、LBA をそのまま READ に渡せば読める
    - 層の先頭ブロックは seed 例外のため raw 経路で読む
    - `src/friidump.c` / `dumper.{c,h}`: `-t/--startsector`・`-e/--stopsector`（部分吸い出し）を追加。
      `-t` 指定時は出力を先頭(0)から書き、既存データのハッシュ再計算をスキップ
    - 実測（Pi Zero + GCC-4240N + Initio）: layerbreak=2,084,960 / 総セクタ=4,155,840。
      層境界 ±32 セクタをまたぐ読み出しは warn=0・EDC 通過
    - 第2層の実データ速度: **method11 が唯一実用**（校正後 ~0.44MB/s）。
      method12 は 0.0002MB/s、method0/2 は 0.017MB/s で実用不可
    - `-e` は「指定セクタを含む」実装（`-e end` は end を含む）。フル吸い出しの bit-exact 検証は
      USB ブリッジの不安定さにより未完（下記「既知の制限」参照）

13. **`rust/friidump/` — C 実装の増分 Rust 移植（進行中）**
    - 純粋コア（`ecma267`/`unscrambler`/`metadata`/`hasher`）、デバイス層（Linux SG_IO）、
      `cache`/`read`/`disc`（method11/12）、`dumper`/`cli`/`main` を移植
    - C から生成したゴールデンベクタ（`tests/vectors`）と仮想ドライブのモックでオフライン検証
    - **Pi 実機で C 版と bit-exact**: method11/12 の ISO/RAW、CLI 吸い出し、`-u`、resume
    - 実機速度は C 版の約 2 倍（同一 4097 セクタで C 85.5s / Rust 40.2s、user 57s / 13s）
    - `-e` は **exclusive に統一**（C 版は inclusive。help 表記に合わせた是正）
    - リファクタ: グローバル可変状態の排除・`Result` 伝播・安全なバッファ/オフセット・
      未初期化変数シフト(R7)の是正・`exit()` 廃止
    - 未実装: method0-10（他ドライブ用）/ Windows SPTI / `-A`
    - ビルド・使い方は `AGENTS.md` の「Rust 移植」節を参照

## 既知の制限・注意

- **Pi Zero W でドライブ接続時に Pi がハング → `dwc_otg.fiq_enable=0` で解消**（2026-09-24）
  - 症状: 接続の瞬間に Pi が固まる（USB 列挙ログすら出ない）。watchdog で再起動するが
    接続されたままだと再ハング。**ユーザ空間到達前の dwc_otg 内で停止**
  - 対策: `/boot/firmware/cmdline.txt` に `dwc_otg.fiq_enable=0` を追加
    - `dwc_otg.fiq_fsm_enable=0` 単独 / `dwc_otg.speed=1` 単独では解消しない
    - 速度影響は無視できる（吸い出し律速 ~0.44MB/s）
  - カーネル/ファームは 2026-09-15 から不変。ソフト回帰ではなくハード/ドライバ相互作用。
    同ブリッジは Windows(xHCI) では正常動作
- **USB ブリッジ（Initio 13FD:1040）は長時間の読み出しで USB から切断する**（最優先の課題）
  - 症状: `usb 1-1: device descriptor read/64, error -110` → `USB disconnect` →
    `usb1-port1: attempt power cycle`（**復帰せず、物理再挿しが必要**）
  - 吸い出し結果が **ゼロで埋まる**（ドライブがゼロを返す）。破損位置は実行ごとに変わる
  - 外部 5V 給電でも発生。**ブリッジ自体（信号品質/ファーム）が原因**の可能性が高く、
    **別の USB-IDE/SATA ブリッジ**を推奨。長時間運用はチャンク分割＋再挿し前提
  - 参考: ディスクの限界領域（傷・汚れ）でも同様にゼロ/データが不安定になることがある
- **`GCC-4240N(E112)` + `Initio 13FD:1040`** は `0xe7` も通常READも通るが、
  GCの連続読み出しでドライブが **READ完了を待たず stale を返す**（単発 READ は無視される）。
  - 対策: **READ を 2 回発行してから E7 を読む**（`method11`/`method12` の前提）
  - **別 USB ブリッジでの改善は不確実**。通常のデータDVDは ~2MB/s 出るが、GC はドライブの
    GC読み挙動（キャッシュが stale になる）が支配的で ~0.3MB/s。詳細は「高速化の結果」参照
- E7 のベースアドレスは機種依存。GCC-4160N/4240N は `0xA13000`（`0x80000000` は誤り）
- **Linux の SG_IO**: ベンダコマンド `0xe7` はブロックデバイス `/dev/srN` では `EPERM`
  （CAP_SYS_RAWIO 必須）。**文字デバイス `/dev/sgN` を `O_RDWR` で開く**こと
- スクランブル seed は **16 ブロック周期**。GC seed は disc ごとに異なるため校正が必要（block0 は例外）
- 通常 READ が返す 2048B は「`raw[12:2060] XOR 固定seed`」であり、各セクタ先頭 6B (`raw[6:12]`) を含まない
  - そのため高速化には先頭 6B を **E7** で補う必要がある
- **SCSI のゼロ長転送（READ12 長さ0）は Windows の SPTI + 本ブリッジでタイムアウト**する
  （`dvd_flush_cache_READ12` を Windows で使わない）

## 高速化の結果

- `0xe7` は **転送律速で上限 ~650 KB/s**（サイズ比例）
- **`method12`（既定、実装済み）**: `streaming READ×2 + E7(先頭12B/末尾10B)×16 + 16位相補正 + 全ブロックEDC`
  - **全ディスク(712880 セクタ)で ISO=redump・RAW=DIC に一致**、~0.32 MB/s（GC 換算 ~62分）
- **持続 16 セクタ/物理READ が上限**（キャッシュは16セクタ境界整列・窓26セクタ）。
  速度設定・窓移動・非同期重畳・READサイズ拡大のいずれでも短縮不可を実測で確認
- **通常のデータDVD**はシーク無しの連続READで ~2MB/s（OS標準読みも同値）。ただしこれは
  ドライブの先読みキャッシュが正しいデータを返すためで、**GC では先読みが stale になり移植不可**
- したがって **GC は ~62分がソフト限界**。30分以内はハード変更前提
  （MT1959系Blu-ray + OmniDrive / 中古 LG GDR-8164B・GDR-8082 + UDMA ブリッジ 等）


## 次回対応予定

- **GCC-4241N / GCC-4242N（DIC の 0xe7 Type2_1 / Type2_2）は下地のみ（`disc_read_sector_13` は未実装スタブ）**
  - DIC（`execScsiCmdforDVD.cpp`）では Type2 は `baseAddr≒0x80000000` を
    `0x2040`（= 4セクタ × 2064B）ずつ回転させ、**4セクタ単位の E7** で 1 ブロックを読む方式
  - 本forkは Type1（`0xA13000` / `method12`）と Type4/3 相当（`0x80000000` 固定 / `method9` 系）を実装済み。
    Type2 は drive_profile に行（`READ_FAMILY_HITACHI_TYPE2` / method13）を追加済みで、
    `GCC-4241N` / `GCC-4242N` は method13 に割当（**読み出し実装は次回**）
  - 次回: 実機でキャッシュ配置・回転量・4セクタE7 の挙動を実測（`tools/probe/` に配置プローブを追加）し、
    `disc_read_sector_13()` を実装、EDC 検証で誤り率を定量化する
  - 注意: DIC 公式 README は 4241N/4242N について「吸い出せるが many errors occurred」と明記


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
