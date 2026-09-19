# tools/probe — 調査用プローブ（Windows SPTI / Python）

GCC-4160N / GCC-4240N（Hitachi MN103S）の `0xe7` 読み出し挙動を調査するための
使い捨てスクリプト群です。`ctypes` で Windows の `IOCTL_SCSI_PASS_THROUGH_DIRECT` を叩きます。

> **注意**: 実行前に DiscImageCreator など、同じ光学ドライブを使用するツールを終了してください。
> ドライブを排他利用できないとコマンドが失敗します。

## 依存・前提

- Windows + Python 3.10 以降（追加パッケージ不要）
- 調査対象の GC ディスクをドライブ（既定 `D:`）に挿入
- raw / iso と比較するスクリプトは、DIC などで取得した
  `gc2.raw`（2064B/セクタ）と `gc2.iso`（2048B/セクタ）を用意し、
  各スクリプト冒頭のパス（`RAW` / `ISO`）を書き換えてください

## スクリプト

| ファイル | 目的 |
| --- | --- |
| `spti_probe.py` | 共通ライブラリ。READ(12)・`0xe7` の発行と応答時間計測 |
| `probe2.py` | streaming READ 後の `0xe7` 連続取得（キャッシュ範囲）確認 |
| `probe3.py` | 通常READの 2048B が plain / スクランブル どちらかを DIC の raw/iso と比較 |
| `probe4.py` | 通常READが「raw XOR 固定seed」で説明できるか確認（seed=0x80 を特定） |
| `probe5.py` | `raw ^ iso` から GC スクランブル seed を探索 |
| `probe6.py` | ブロックごとの GC seed（16ブロック周期）を確認 |
| `probe7.py` | seed 探索（オフセット・バイトスワップ） |
| `probe8.py` | ブロック内の per-sector seed を確認 |
| `probe10.py` | スクランブル補正の周期性（256セクタ周期か）を確認 |
| `probe11.py` | seed 全探索（16bit） |
| `probe12.py` | `raw ^ iso` 後半が LFSR か（先頭6Bズレ仮説）確認 |
| `probe14.py` | READ CD (0xBE) の各種フラグで 2064 生フレームが取れるか |
| `probe15.py` | streaming READ 後の E7 6B×16 で各セクタ先頭6Bが取れるか |
| `probe16.py` | fast方式プロトタイプ（README/PATCHES 参照） |
| `probe17.py` | fast方式の連続ブロック成立率測定 |

## 得られた知見（要約）

- E7 のベースアドレスは **`0xA13000`**、転送は **~682 KB/s** でサイズ比例（レイテンシ律速ではない）
- 通常 READ(12) は **streaming ビット（byte10=0x80）** を立てると成功し、
  `raw[12:2060] XOR LFSR(seed)` を返す（seed はブロック毎に変化、16ブロック周期）
- 各セクタ先頭 6B (`raw[6:12]`) は通常 READ に含まれないため、**E7（12B）で補う**必要がある
- 連続読み出しでは **READ を 2 回発行**しないとキャッシュが更新されない（Initio ブリッジ）
- fast方式: `streaming READ×2 + E7(先頭12B)×16 + GC/ドライブ両 seed 復号` で 112/112 一致

これらは `PATCHES.md` にも記載しています。
