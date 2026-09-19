# tools/probe — 読み出し調査プローブ（Windows SPTI / Python）

GCC-4240N（Hitachi MN103S）の `0xe7` / 通常READ の挙動を調査するための
使い捨て Python スクリプト群です。Windows の `ctypes` で
`IOCTL_SCSI_PASS_THROUGH_DIRECT` を直接発行します。

## 注意

- 実行前に **DiscImageCreator 等、同じドライブを使うツールを終了**すること（排他利用のため）
- raw/iso と比較するスクリプトは、DIC 等で取得した `gc2.raw`(2064B/セクタ) と
  `gc2.iso`(2048B/セクタ) を用意し、各スクリプト冒頭のパスを環境に合わせて変更する
- 既定デバイスは `D:`

## 主なスクリプト

| ファイル | 目的 |
| --- | --- |
| `spti_probe.py` | 共通ライブラリ（READ(12)・`0xe7` の発行、応答時間計測） |
| `probe3.py` / `probe4.py` | 通常READの内容判定（plain / スクランブル、固定seed 0x80 の特定） |
| `probe5.py`〜`probe8.py` | GC スクランブル seed の探索・16ブロック周期の確認 |
| `probe10.py`〜`probe12.py` | 周期性・seed 全探索・先頭6Bズレ仮説の検証 |
| `probe14.py` | READ CD (0xBE) で生フレームが取れるか |
| `probe15.py` | streaming READ 後の `0xe7` 6B×16 で各セクタ先頭6B取得 |
| `probe16.py` / `probe17.py` | fast方式プロトタイプと連続ブロック成立率（※`probe16` は単発READのため現在のブリッジではデシンクし全滅する） |
| `usb_speed.py` | USB接続速度（Low/Full/High/Super）を SetupAPI で取得 |
| `bench.py` | E2/E4/E5: READ/E7 レイテンシと方式別サイクル比較 |
| `probe_timing.py` | ブロック内訳（trigger READ / settle READ / E7） |
| `probe_cache.py` / `probe_alignment.py` | キャッシュ窓（26セクタ）と16セクタ整列 |
| `probe_lag.py` / `probe_read26.py` / `probe_offset_read.py` | キャッシュ遅延・READ(26)・LBA整列の調査 |
| `probe_prefetch.py` / `probe_final.py` | PREFETCH/sleep 等の確定方法比較（※ゼロ長転送を使う版はハング注意） |
| `probe_stride.py` / `probe_stride2.py` | 校正 `corr` を使った READ(26)・ストライドの完全性検証 |
| `probe_speed_modes.py` | SET CD SPEED / SET STREAMING / READ(10) / FUA と E7最適化 |
| `probe_invalidate.py` | SYNC_CACHE / SEEK(10) による窓移動の可否 |
| `probe_async.py` | overlapped SPTI による非同期重畳の検証 |
| `probe_profile.py` / `probe_radius.py` / `probe_total.py` | CAV・半径依存・総コスト（全ディスク時間） |
| `probe_normal.py` / `probe_normal2.py` | 通常DVDでの切り分け（GC固有か基板起因か） |
| `probe_gc_size.py` / `probe_gc_seq.py` / `probe_gc_force.py` | GCのREADサイズ別復元数・単発READの挙動 |
| `probe_seek.py` | SEEKバリア方式の持続検証 |
| `probe_drop.py` | メディア取りこぼし（E7/READ連発）の切り分け |

## 得られた知見（要約）

- `0xe7` のベースアドレスは **`0xA13000`**（`0x80000000` は誤り）。E7 転送は ~650 KB/s
- 通常 READ(12) は **streaming ビット（byte10=0x80）** で成功し、`raw[12:2060] XOR LFSR(seed)` を返す
- 各セクタ先頭 6B (`raw[6:12]`) は通常READに含まれず **E7 必須**
- 実READは**転送サイズ非依存で ~80ms 固定**。ブロック毎 = trigger 13ms + settle 80ms + E7×16 11.6ms ≒ 105ms
- キャッシュは **16セクタ境界整列・26セクタ窓**。連続READは窓が重なりデシンク → **実質2READ/ブロック**
- **PREFETCH(0x34) は ILLEGAL REQUEST で拒否**（無効）
- **USBリンクは High-Speed**。律速はブリッジの固定レイテンシ
- **通常盤は~2MB/s出るがGCは~0.3MB/s**（GCはドライブの先読みがstaleになり移植不可）
- 詳細は `PATCHES.md`（本fork）を参照
