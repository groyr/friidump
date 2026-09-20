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

/* 1 ブロックを mn103s の raw 経路で読み、host データ(rd)と復号済みデータ(P)を返す。
 * READ は 2 回必要（1 回目は吸収される）。rawout が非 NULL なら生フレームもコピーする。 */
static bool disc_fast_read_raw_block (disc *d, u_int32_t blk, u_int8_t *rd, u_int8_t *rawout, u_int8_t *P) {
	static u_int8_t rawbuf[RAW_BLOCK_SIZE];
	u_int32_t lba = blk * SECTORS_PER_BLOCK;
	int retry;

	for (retry = 0; retry < 5; retry++) {
		u_int32_t k;
		bool ok = true;

		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, NULL, 0) < 0)
			continue;
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, rd, BLOCK_SIZE) < 0)
			continue;
		if (dvd_memdump (d -> dvd, 0, 1, RAW_BLOCK_SIZE, rawbuf) < 0)
			continue;

		for (k = 0; k < SECTORS_PER_BLOCK; k++) {
			u_int32_t sn = ((u_int32_t) rawbuf[k * RAW_SECTOR_SIZE + 1] << 16)
				| ((u_int32_t) rawbuf[k * RAW_SECTOR_SIZE + 2] << 8)
				| rawbuf[k * RAW_SECTOR_SIZE + 3];
			if (sn != lba + k + 0x30000) { ok = false; break; }
		}
		if (!ok)
			continue;
		/* rawout には unscramble 前の生フレーム（真スクランブル像）を返す */
		if (rawout)
			memcpy (rawout, rawbuf, RAW_BLOCK_SIZE);
		if (!unscrambler_unscramble_16sectors (d -> u, lba, rawbuf, P))
			continue;
		return (true);
	}
	return (false);
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

int disc_read_sector_11 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t out[BLOCK_SIZE];
	static u_int8_t rawbuf[RAW_BLOCK_SIZE];
	u_int32_t blk = sector_no / SECTORS_PER_BLOCK;
	u_int32_t lba = blk * SECTORS_PER_BLOCK;
	int retry;

	memset (rawbuf, 0, sizeof (rawbuf));	/* fast は raw 出力非対応（合成のみ） */

	/* block0 は seed が例外なので raw 経路で読む */
	if (blk == 0) {
		if (!disc_fast_read_raw_block (d, 0, rd, NULL, out)) {
			error ("fast read failed at block 0");
			return (false);
		}
		disc_cache_add_block (d, 0, out, rawbuf);
		return (true);
	}

	if (!disc_fast_calibrate (d))
		return (false);

	for (retry = 0; retry < 5; retry++) {
		u_int32_t k, i;
		int m = blk % 16;
		bool ok = true;

		/* READ は 2 回必要（1 回目は吸収される）。PREFETCH(0x34) は
		 * 本ドライブで ILLEGAL REQUEST のため使わず、常に READ×2 とする。 */
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, NULL, 0) < 0) { ok = false; continue; }
		if (dvd_read_sector_streaming (d -> dvd, lba, NULL, rd, BLOCK_SIZE) < 0) { ok = false; continue; }

		for (k = 0; k < SECTORS_PER_BLOCK; k++) {
			u_int8_t e[12];
			u_int32_t sn;

			if (dvd_memdump (d -> dvd, k * RAW_SECTOR_SIZE, 1, 12, e) < 0) { ok = false; break; }
			sn = ((u_int32_t) e[1] << 16) | ((u_int32_t) e[2] << 8) | e[3];
			if (sn != lba + k + 0x30000) { ok = false; break; }

			memcpy (out + k * SECTOR_SIZE, e + 6, 6);	/* 先頭 6B は raw から */
			for (i = 0; i < SECTOR_SIZE - 6; i++)
				out[k * SECTOR_SIZE + 6 + i] = rd[k * SECTOR_SIZE + i] ^ d -> fast_corr[m][i];
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

int disc_read_sector_12 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	static u_int8_t rd[BLOCK_SIZE];
	static u_int8_t rawb[RAW_BLOCK_SIZE];
	static u_int8_t rawtrue[RAW_BLOCK_SIZE];
	static u_int8_t out[BLOCK_SIZE];
	u_int32_t blk = sector_no / SECTORS_PER_BLOCK;
	u_int32_t lba = blk * SECTORS_PER_BLOCK;
	int retry;

	/* block0 は seed が例外なので raw 経路（EDC検証つき）で読む */
	if (blk == 0) {
		if (!disc_fast_read_raw_block (d, 0, rd, rawtrue, out)) {
			error ("method12 read failed at block 0");
			return (false);
		}
		disc_cache_add_block_raw (d, 0, out, rawtrue);
		return (true);
	}

	if (!disc_fast_calibrate12 (d))
		return (false);

	for (retry = 0; retry < 5; retry++) {
		u_int32_t k, i;
		int m = blk % 16;
		bool ok = true;

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
			if (sn != lba + k + 0x30000) { ok = false; break; }
			/* 生フレーム末尾 10B（CPR_MAI 6B + EDC 4B） */
			if (dvd_memdump (d -> dvd, k * RAW_SECTOR_SIZE + 2054, 1, 10, tail) < 0) { ok = false; break; }

			memcpy (rawb + k * RAW_SECTOR_SIZE, head, 12);
			for (i = 0; i < SECTOR_SIZE; i++)
				rawb[k * RAW_SECTOR_SIZE + 12 + i] = rd[k * SECTOR_SIZE + i] ^ d -> drive_cipher12[m][i];
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
