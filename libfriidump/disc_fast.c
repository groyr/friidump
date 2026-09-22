/***************************************************************************
 *   Copyright (C) 2007 by Arep                                            *
 *   Support is provided through the forums at                             *
 *   http://wii.console-tribe.com                                          *
 *                                                                         *
 *   This program is free software; you can redistribute it and/or modify  *
 *   it under the terms of the GNU General Public License as published by  *
 *   the Free Software Foundation; either version 2 of the License, or     *
 *   (at your option) any later version.                                   *
 *                                                                         *
 *   This program is distributed in the hope that it will be useful,       *
 *   but WITHOUT ANY WARRANTY; without even the implied warranty of        *
 *   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
 *   GNU General Public License for more details.                          *
 *                                                                         *
 *   You should have received a copy of the GNU General Public License     *
 *   along with this program; if not, write to the                         *
 *   Free Software Foundation, Inc.,                                       *
 *   59 Temple Place - Suite 330, Boston, MA  02111-1307, USA.             *
 ***************************************************************************/

/*! \file
 * \brief Hitachi MN103S (GCC-4160N/4240N) の fast 読み出し方式（method11/12）。
 *
 * 通常 READ(12) が返すホストデータと、E7 で取得した生フレームの関係を代表16ブロックで
 * 校正し、以降は READ×2 + E7(12B) で生フレームを復元する。method12 はさらに全ブロックを
 * EDC 検証し、raw をマスターとして保存する。
 */

#include "misc.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "constants.h"
#include "disc_internal.h"


/* ---------- fast方式 (method11) ----------
 * 通常 READ(12)(streaming) のホストデータは raw[12:2060] XOR cipher(drive_seed) を返す。
 * GC 側 seed・ドライブ側 seed はともに 16 ブロック周期なので、代表 16 ブロックで
 *   corr[m][i] = cipher(drive_seed_m)[i] XOR cipher(gc_seed_m)[i]
 * を求めておけば、以降は
 *   P[6:2048] = rd[0:2042] XOR corr[m][0:2042]   (m = block mod 16)
 * で復号できる。各セクタ先頭 6B (raw[6:12]) は通常 READ に含まれないため E7(12B) で補う。
 */

/* 層検出・層別オフセット（前方宣言） */
static u_int32_t disc_fast_sn_offset (disc *d, u_int32_t blk);
static bool disc_fast_detect_layer2 (disc *d);

/* 1 ブロックの生フレームを読む（層の物理アクセス補正込み）。
 * READ は 2 回必要（1 回目は吸収される）。rawout が非 NULL なら生フレームもコピーする。
 * method12 と共用するため、この関数は EDC 解除を行わない。 */
static bool disc_fast_fetch_raw (disc *d, u_int32_t blk, u_int8_t *rd, u_int8_t *rawout) {
	static u_int8_t rawbuf[RAW_BLOCK_SIZE];
	u_int32_t lba = blk * SECTORS_PER_BLOCK;
	int retry;

	for (retry = 0; retry < 5; retry++) {
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, NULL, 0) < 0)
			continue;
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, rd, BLOCK_SIZE) < 0)
			continue;
		if (dvd_memdump (d -> dvd, 0, 1, RAW_BLOCK_SIZE, rawbuf) < 0)
			continue;
		if (rawout)
			memcpy (rawout, rawbuf, RAW_BLOCK_SIZE);
		return (true);
	}
	return (false);
}

/* 1 ブロックを mn103s の raw 経路で読み、host データ(rd)と復号済みデータ(P)を返す。 */
static bool disc_fast_read_raw_block (disc *d, u_int32_t blk, u_int8_t *rd, u_int8_t *rawout, u_int8_t *P) {
	static u_int8_t rawbuf[RAW_BLOCK_SIZE];
	u_int32_t lba = blk * SECTORS_PER_BLOCK;

	if (!disc_fast_fetch_raw (d, blk, rd, rawbuf))
		return (false);
	/* rawout には unscramble 前の生フレーム（真スクランブル像）を返す */
	if (rawout)
		memcpy (rawout, rawbuf, RAW_BLOCK_SIZE);
	if (!unscrambler_unscramble_16sectors (d -> u, lba, rawbuf, P))
		return (false);
	return (true);
}

/* 代表 16 ブロック(16..31)から補正テーブルを作る */
static bool disc_fast_calibrate (disc *d) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t P[BLOCK_SIZE];
	int m;

	if (d -> fast_ready)
		return (true);

	for (m = 0; m < 16; m++) {
		u_int32_t k, i;
		if (!disc_fast_read_raw_block (d, 16 + m, rd, NULL, P)) {
			error ("fast calibration failed at block %d", 16 + m);
			return (false);
		}
		for (k = 0; k < SECTORS_PER_BLOCK; k++)
			for (i = 0; i < SECTOR_SIZE - 6; i++)
				d -> fast_corr[m][i] = rd[k * SECTOR_SIZE + i] ^ P[k * SECTOR_SIZE + 6 + i];
	}
	d -> fast_ready = true;
	debug ("fast calibration done (16 phases)");
	return (true);
}

/* 第2層用の method11 補正テーブルを作る。第2層の代表 16 ブロックを使う。 */
static bool disc_fast_calibrate_layer2 (disc *d) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t P[BLOCK_SIZE];
	u_int32_t base_blk;
	int m;

	if (d -> fast_ready2)
		return (true);
	if (!disc_fast_detect_layer2 (d))
		return (false);

	base_blk = ((d -> layerbreak / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK) / SECTORS_PER_BLOCK + 16;

	for (m = 0; m < 16; m++) {
		u_int32_t k, i;
		if (!disc_fast_read_raw_block (d, base_blk + m, rd, NULL, P)) {
			error ("method11 layer2 calibration failed at block %u", base_blk + m);
			return (false);
		}
		for (k = 0; k < SECTORS_PER_BLOCK; k++)
			for (i = 0; i < SECTOR_SIZE - 6; i++)
				d -> fast_corr2[m][i] = rd[k * SECTOR_SIZE + i] ^ P[k * SECTOR_SIZE + 6 + i];
	}
	d -> fast_ready2 = true;
	debug ("method11 layer2 calibration done (16 phases)");
	return (true);
}

int disc_read_sector_11 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t out[BLOCK_SIZE];
	static u_int8_t rawbuf[RAW_BLOCK_SIZE];
	u_int32_t blk = sector_no / SECTORS_PER_BLOCK;
	u_int32_t lba = blk * SECTORS_PER_BLOCK;
	int retry;
	bool layer2 = false;
	u_int8_t (*corr)[2042];

	memset (rawbuf, 0, sizeof (rawbuf));	/* fast は raw 出力非対応（合成のみ） */

	if (d -> layerbreak > 0 && d -> type == DISC_TYPE_WII_DL && lba >= d -> layerbreak)
		layer2 = true;

	/* block0 と第2層先頭ブロックは seed が例外なので raw 経路で読む */
	if (blk == 0 || (layer2 && lba == (d -> layerbreak / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK)) {
		if (!disc_fast_read_raw_block (d, blk, rd, NULL, out)) {
			error ("fast read failed at block %u", blk);
			return (false);
		}
		disc_cache_add_block (d, blk, out, rawbuf);
		return (true);
	}

	if (layer2) {
		if (!disc_fast_calibrate_layer2 (d))
			return (false);
		corr = d -> fast_corr2;
	} else {
		if (!disc_fast_calibrate (d))
			return (false);
		corr = d -> fast_corr;
	}

	for (retry = 0; retry < 5; retry++) {
		u_int32_t k, i;
		int m = blk % 16;
		bool ok = true;
		u_int32_t sn_off = disc_fast_sn_offset (d, blk);

		/* READ は 2 回必要（1 回目は吸収される）。PREFETCH(0x34) は
		 * 本ドライブで ILLEGAL REQUEST のため使わず、常に READ×2 とする。 */
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, NULL, 0) < 0) { ok = false; continue; }
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, rd, BLOCK_SIZE) < 0) { ok = false; continue; }

		for (k = 0; k < SECTORS_PER_BLOCK; k++) {
			u_int8_t e[12];
			u_int32_t sn;

			if (dvd_memdump (d -> dvd, k * RAW_SECTOR_SIZE, 1, 12, e) < 0) { ok = false; break; }
			sn = ((u_int32_t) e[1] << 16) | ((u_int32_t) e[2] << 8) | e[3];
			if (sn != lba + k + sn_off) { ok = false; break; }

			memcpy (out + k * SECTOR_SIZE, e + 6, 6);	/* 先頭 6B は raw から */
			for (i = 0; i < SECTOR_SIZE - 6; i++)
				out[k * SECTOR_SIZE + 6 + i] = rd[k * SECTOR_SIZE + i] ^ corr[m][i];
		}
		if (ok) {
			disc_cache_add_block (d, blk, out, rawbuf);
			return (true);
		}
		warning ("fast read retry %d for block %u", retry + 1, blk);
	}

	warning ("fast read failed for block %u, falling back to raw", blk);
	if (disc_fast_read_raw_block (d, blk, rd, NULL, out)) {
		disc_cache_add_block (d, blk, out, rawbuf);
		return (true);
	}
	error ("fast read failed at block %u", blk);
	return (false);
}


/* ---------- method12: rawマスター + 全ブロックEDC検証 ----------
 * method11 と同じく host データ(rd)から fast 復号するが、加えて
 *   ・E7 で各セクタの生フレーム先頭12B(raw[0:12])と末尾10B(raw[2054:2064])を取得
 *   ・raw[12:2060] = rd XOR drive_cipher12[位相] で復元
 * して生フレームを組み立て、unscrambler に渡して EDC 検証する。
 * これにより raw をマスターとして保存でき、かつ全ブロックを EDC で検証できる。
 * 速度は E7 が 1→2 回/セクタに増える分だけ method11 より僅かに遅い。 */

/* 層に応じた生フレームのセクタ番号オフセットを返す。
 * 第1層: sn = LBA + 0x30000。
 * 第2層: sn = LBA + layer_sn_offset2（この関数を呼ぶ前に検出済みであること）。 */
static u_int32_t disc_fast_sn_offset (disc *d, u_int32_t blk) {
	u_int32_t lba = blk * SECTORS_PER_BLOCK;
	if (d -> layerbreak > 0 && d -> type == DISC_TYPE_WII_DL && lba >= d -> layerbreak)
		return (d -> layer_sn_offset2);
	return (0x30000);
}

/* 第2層の先頭ブロックを読み、sn - LBA のオフセット layer_sn_offset2 を検出する。
 * 第2層は第1層と物理セクタ番号の対応が異なる（実測: 一定のオフセット）。 */
static bool disc_fast_detect_layer2 (disc *d) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t rawbuf[RAW_BLOCK_SIZE];
	u_int32_t start = (d -> layerbreak / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK;
	u_int32_t sn;

	if (d -> layer2_ready)
		return (true);
	if (d -> layerbreak == 0 || start == 0)
		return (false);	/* 単層ディスク */

	if (!disc_fast_fetch_raw (d, start / SECTORS_PER_BLOCK, rd, rawbuf)) {
		error ("layer2 detect failed at block %u", start / SECTORS_PER_BLOCK);
		return (false);
	}
	sn = ((u_int32_t) rawbuf[1] << 16) | ((u_int32_t) rawbuf[2] << 8) | rawbuf[3];
	d -> layer_sn_offset2 = sn - start;
	d -> layer2_ready = true;
	debug ("layer2 detected: lba=%u sn=0x%X offset2=0x%X", start, sn, d -> layer_sn_offset2);
	return (true);
}

/* 代表 16 ブロック(16..31)から drive_cipher12 を作る */
static bool disc_fast_calibrate12 (disc *d) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t rawb[RAW_BLOCK_SIZE];
	static u_int8_t P[BLOCK_SIZE];
	int m;

	if (d -> fast12_ready)
		return (true);

	for (m = 0; m < 16; m++) {
		u_int32_t k, i;
		if (!disc_fast_read_raw_block (d, 16 + m, rd, rawb, P)) {
			error ("method12 calibration failed at block %d", 16 + m);
			return (false);
		}
		for (k = 0; k < SECTORS_PER_BLOCK; k++)
			for (i = 0; i < SECTOR_SIZE; i++)
				d -> drive_cipher12[m][i] = rd[k * SECTOR_SIZE + i] ^ rawb[k * RAW_SECTOR_SIZE + 12 + i];
	}
	d -> fast12_ready = true;
	debug ("method12 calibration done (16 phases)");
	return (true);
}

/* 第2層用の校正テーブルを作る（第2層の代表 16 ブロックを使う）。
 * 第2層は第1層とドライブ側の逆スクランブル鍵が異なるため、別に校正する。 */
static bool disc_fast_calibrate12_layer2 (disc *d) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t rawb[RAW_BLOCK_SIZE];
	static u_int8_t P[BLOCK_SIZE];
	u_int32_t base_blk;
	int m;

	if (d -> fast12_ready2)
		return (true);
	if (!disc_fast_detect_layer2 (d))
		return (false);

	/* 第2層の先頭から 16 ブロック分を校正に使う（境界直後は避けて +16 から） */
	base_blk = ((d -> layerbreak / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK) / SECTORS_PER_BLOCK + 16;

	for (m = 0; m < 16; m++) {
		u_int32_t k, i;
		if (!disc_fast_read_raw_block (d, base_blk + m, rd, rawb, P)) {
			error ("method12 layer2 calibration failed at block %u", base_blk + m);
			return (false);
		}
		for (k = 0; k < SECTORS_PER_BLOCK; k++)
			for (i = 0; i < SECTOR_SIZE; i++)
				d -> drive_cipher12_2[m][i] = rd[k * SECTOR_SIZE + i] ^ rawb[k * RAW_SECTOR_SIZE + 12 + i];
	}
	d -> fast12_ready2 = true;
	debug ("method12 layer2 calibration done (16 phases)");
	return (true);
}

int disc_read_sector_12 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t rawb[RAW_BLOCK_SIZE];
	static u_int8_t rawtrue[RAW_BLOCK_SIZE];
	static u_int8_t out[BLOCK_SIZE];
	u_int32_t blk = sector_no / SECTORS_PER_BLOCK;
	u_int32_t lba = blk * SECTORS_PER_BLOCK;
	int retry;
	bool layer2 = false;
	u_int8_t (*cipher)[2048];

	/* block0 は seed が例外なので raw 経路（EDC検証つき）で読む。
	 * 第2層の先頭ブロックも層替わりで例外の可能性があるため raw 経路で読む。 */
	if (d -> layerbreak > 0 && d -> type == DISC_TYPE_WII_DL && lba >= d -> layerbreak)
		layer2 = true;

	if (blk == 0 || (layer2 && lba == (d -> layerbreak / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK)) {
		if (!disc_fast_read_raw_block (d, blk, rd, rawtrue, out)) {
			error ("method12 read failed at block %u", blk);
			return (false);
		}
		disc_cache_add_block_raw (d, blk, out, rawtrue);
		return (true);
	}

	if (layer2) {
		if (!disc_fast_calibrate12_layer2 (d))
			return (false);
		cipher = d -> drive_cipher12_2;
	} else {
		if (!disc_fast_calibrate12 (d))
			return (false);
		cipher = d -> drive_cipher12;
	}

	for (retry = 0; retry < 5; retry++) {
		u_int32_t k, i;
		int m = blk % 16;
		bool ok = true;
		u_int32_t sn_off = disc_fast_sn_offset (d, blk);

		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, NULL, 0) < 0)
			continue;
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, rd, BLOCK_SIZE) < 0)
			continue;

		memset (rawb, 0, RAW_BLOCK_SIZE);
		for (k = 0; k < SECTORS_PER_BLOCK; k++) {
			u_int8_t head[12], tail[10];
			u_int32_t sn;

			/* 生フレーム先頭 12B（ID/IED/CPR_MAI） */
			if (dvd_memdump (d -> dvd, k * RAW_SECTOR_SIZE, 1, 12, head) < 0) { ok = false; break; }
			sn = ((u_int32_t) head[1] << 16) | ((u_int32_t) head[2] << 8) | head[3];
			if (sn != lba + k + sn_off) { ok = false; break; }
			/* 生フレーム末尾 10B（CPR_MAI 6B + EDC 4B） */
			if (dvd_memdump (d -> dvd, k * RAW_SECTOR_SIZE + 2054, 1, 10, tail) < 0) { ok = false; break; }

			memcpy (rawb + k * RAW_SECTOR_SIZE, head, 12);
			for (i = 0; i < SECTOR_SIZE; i++)
				rawb[k * RAW_SECTOR_SIZE + 12 + i] = rd[k * SECTOR_SIZE + i] ^ cipher[m][i];
			memcpy (rawb + k * RAW_SECTOR_SIZE + 2054, tail, 10);
		}
		if (!ok) {
			warning ("method12 retry %d for block %u (memdump/sector)", retry + 1, blk);
			continue;
		}

		/* EDC 検証（rawb は unscramble で一部書き換わるため、生フレームを退避） */
		memcpy (rawtrue, rawb, RAW_BLOCK_SIZE);
		if (!unscrambler_unscramble_16sectors (d -> u, lba, rawb, out)) {
			warning ("method12 EDC failed for block %u (retry %d)", blk, retry + 1);
			continue;
		}
		disc_cache_add_block_raw (d, blk, out, rawtrue);
		return (true);
	}

	warning ("method12 failed for block %u, falling back to raw", blk);
	if (disc_fast_read_raw_block (d, blk, rd, rawtrue, out)) {
		disc_cache_add_block_raw (d, blk, out, rawtrue);
		return (true);
	}
	error ("method12 read failed at block %u", blk);
	return (false);
}
