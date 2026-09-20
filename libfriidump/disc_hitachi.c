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
 * \brief Hitachi MN103 系ドライブの読み出し方式（method7-10, method13）。
 *
 * いずれも 0xe7 によるドライブ内メモリダンプで生フレームを取得する。
 * キャッシュのベースアドレス・配置は機種（drive_profile）依存。
 */

#include "misc.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "constants.h"
#include "disc_internal.h"


int disc_read_sector_7 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	bool out;
	u_int32_t start_block;
	int j, ret, retry;
	u_int8_t buf[5][16 * 2064];
	u_int8_t buf_unscrambled[5][16 * 2048];
//fprintf (stdout,"disc_read_sector_7");
	start_block = sector_no / SECTORS_PER_BLOCK;

	out = false;
	for (retry = 0; !out && retry < MAX_READ_RETRIES; retry++) {
		/* Assume everything will turn out well */
		out = true;

		if (retry > 0) {
			warning ("Read retry %d for sector %u", retry, sector_no);

			/* Try to reset in-memory data by seeking to a distant sector */
//			if (sector_no > 1000)
//				dvd_read_sector_streaming (d -> dvd, 0, NULL, NULL, 0);
//			else
//				dvd_read_sector_streaming (d -> dvd, 1500, NULL, NULL, 0);
			if (sector_no +992 +16 <= d -> sectors_no) //smaller than last sector
				dvd_read_sector_dummy (d -> dvd, sector_no +992, 16, NULL, NULL, 0);
			else if (sector_no -992 >= 0)             //larger than first sector
				dvd_read_sector_dummy (d -> dvd, sector_no -992, 16, NULL, NULL, 0);
			else dvd_flush_cache_READ12 (d -> dvd, sector_no, NULL);
		}

		if ((ret = dvd_read_sector_streaming (d -> dvd, sector_no, NULL, NULL, 0)) >= 0) {
			/* 5ブロック分のキャッシュはメモリ上で連続しているため、E7(0xe7)を
			 * 65535バイト以下の読み出しに束ねてコマンド数を削減する（5回→3回）。 */
			int nblk = 0;
			while (nblk < 5 && sector_no + nblk * 16 < d -> sectors_no)
				nblk++;

			if (nblk > 0 && out) {
				u_int32_t total = (u_int32_t) nblk * 16 * 2064;
				u_int32_t done = 0;
				u_int8_t *dst = &buf[0][0];
				while (done < total) {
					u_int32_t chunk = total - done;
					if (chunk > 65535)
						chunk = 65535;	/* Hitachiメモリダンプの上限 */
					if (dvd_memdump (d -> dvd, done, 1, chunk, dst + done) < 0) {
						error ("Memdump failed");
						out = false;
						retry = MAX_READ_RETRIES;		/* Well, if this fails going on is useless */
						break;
					}
					done += chunk;
				}
			}

			for (j = 0; j < nblk && out; j++) {
#ifdef DEBUG
				if (d -> unscrambling) {
#endif
					/* Try to unscramble all data to see if EDC fails */
					if (!unscrambler_unscramble_16sectors (d -> u, sector_no + (j * 16), buf[j], buf_unscrambled[j]))
						out = false;
#ifdef DEBUG
				}
#endif
			}

			if (out) {
				/* It seems all data was unscrambled correctly, so cache them out */
				for (j = 0; j < nblk; j++)
					disc_cache_add_block (d, start_block + j, buf_unscrambled[j], buf[j]);

			}
		} else {
			error ("dvd_read_sector_streaming() failed with %d", ret);
			out = false;
		}
	}

	if (!out)
		error ("Too many retries, giving up");

	return (out);
}


int disc_read_sector_8 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	bool out;
	u_int32_t ram_offset;
	int j, k, ret, retry;
	u_int8_t *sect, buf[5][RAW_BLOCK_SIZE];
	u_int8_t readbuf[BLOCK_SIZE];
	u_int8_t buf_unscrambled[5][BLOCK_SIZE];
	u_int32_t start_block;
//fprintf (stdout,"disc_read_sector_8");
	start_block = sector_no / SECTORS_PER_BLOCK;

	out = false;
	for (retry = 0; !out && retry < MAX_READ_RETRIES; retry++) {
		/* Assume everything will turn out well */
		out = true;

		if (retry > 0) {
			warning ("Read retry %d for sector %u", retry, sector_no);

			/* Try to reset in-memory data by seeking to a distant sector */
//			if (sector_no > 1000)
//				dvd_read_sector_streaming (d -> dvd, 0, NULL, NULL, 0);
//			else
//				dvd_read_sector_streaming (d -> dvd, 1500, NULL, NULL, 0);
			if (sector_no +992 +16 <= d -> sectors_no) //smaller than last sector
				dvd_read_sector_dummy (d -> dvd, sector_no +992, 16, NULL, NULL, 0);
			else if (sector_no -992 >= 0)             //larger than first sector
				dvd_read_sector_dummy (d -> dvd, sector_no -992, 16, NULL, NULL, 0);
			else dvd_flush_cache_READ12 (d -> dvd, sector_no, NULL);
		}

		/* First READ command, this will cache 5 16-sector blocks. Immediately dump relevant data */
		if (sector_no > d -> sectors_no - 1000)
			dvd_read_sector_streaming (d -> dvd, sector_no - 16 * 5 * 2, NULL, NULL, 0);
		else
			dvd_read_sector_streaming (d -> dvd, sector_no + 16 * 5, NULL, NULL, 0);
		if ((ret = dvd_read_sector_streaming (d -> dvd, sector_no, NULL, readbuf, sizeof (readbuf))) >= 0) {
			for (j = 0; j < 5 && sector_no + j * 16 < d -> sectors_no && out; j++) {
				/* Reconstruct raw sectors */
				for (k = 0; k < 16; k++) {
					sect = &buf[j][k * RAW_SECTOR_SIZE];
					ram_offset = (j * RAW_BLOCK_SIZE) + k * RAW_SECTOR_SIZE;
					/* Get first 12 bytes (ID. IED and CPR_MAI fields) and last 4 bytes (EDC field) with memdump */
					if (dvd_memdump (d -> dvd, ram_offset, 1, 12, sect) < 0) {
						error ("Memdump (1) failed");
						out = false;
						retry = MAX_READ_RETRIES;		/* Well, if this fails going on is useless */
					} else if (dvd_memdump (d -> dvd, ram_offset + 2060, 1, 4, sect + 2060) < 0) {	/* Dumping in a single block is faster */
						error ("Memdump (2) failed");
						out = false;
					}
				}
			}

			/* Now the same for remaining 4 16-sector blocks */
			for (j = 0; j < 5 && sector_no + j * 16 < d -> sectors_no && out; j++) {
				if (j == 0 || (ret = dvd_read_sector_streaming (d -> dvd, sector_no + j * 16, NULL, readbuf, sizeof (readbuf))) >= 0) {
					/* Copy "user data" field which has been incorrectly unscrambled by the DVD drive firmware */
					for (k = 0; k < 16; k++) {
						sect = &buf[j][k * RAW_SECTOR_SIZE];
						memcpy (sect + 12, readbuf + k * SECTOR_SIZE, SECTOR_SIZE);
					}
#ifdef DEBUG
					if (d -> unscrambling) {
#endif
						/* Try to unscramble all data to see if EDC fails */
						if (!unscrambler_unscramble_16sectors (d -> u, sector_no + (j * 16), buf[j], buf_unscrambled[j]))
							out = false;
#ifdef DEBUG
					}
#endif
				} else {
					error ("dvd_read_sector_streaming() failed with %d", ret);
					out = false;
				}
			}

			if (out) {
				/* It seems all data were unscrambled correctly, so cache them out */
				for (j = 0; j < 5 && sector_no + j * SECTORS_PER_BLOCK < d -> sectors_no; j++)
					disc_cache_add_block (d, start_block + j, buf_unscrambled[j], buf[j]);
			}
		} else {
			error ("dvd_read_sector_streaming() failed with %d", ret);
			out = false;
		}
	}

	if (!out)
		error ("Too many retries, giving up");

	return (out);
}


int disc_read_sector_9 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	bool out;
	u_int32_t ram_offset;
	int j, k, ret, retry;
	u_int8_t *sect, buf[5][RAW_BLOCK_SIZE];
	u_int8_t readbuf[BLOCK_SIZE], tmp[16];
	u_int8_t buf_unscrambled[5][BLOCK_SIZE];
	u_int32_t start_block;
//fprintf (stdout,"disc_read_sector_9");
	start_block = sector_no / SECTORS_PER_BLOCK;

	out = false;
	for (retry = 0; !out && retry < MAX_READ_RETRIES; retry++) {
		/* Assume everything will turn out well */
		out = true;

		if (retry > 0) {
			warning ("Read retry %d for sector %u", retry, sector_no);

			/* Try to reset in-memory data by seeking to a distant sector */
//			if (sector_no > 1000)
//				dvd_read_sector_streaming (d -> dvd, 0, NULL, NULL, 0);
//			else
//				dvd_read_sector_streaming (d -> dvd, 1500, NULL, NULL, 0);
			if (sector_no +992 +16 <= d -> sectors_no) //smaller than last sector
				dvd_read_sector_dummy (d -> dvd, sector_no +992, 16, NULL, NULL, 0);
			else if (sector_no -992 >= 0)             //larger than first sector
				dvd_read_sector_dummy (d -> dvd, sector_no -992, 16, NULL, NULL, 0);
			else dvd_flush_cache_READ12 (d -> dvd, sector_no, NULL);
		}

		/* First READ command, this will cache 5 16-sector blocks. Immediately dump relevant data */
		if (sector_no > d -> sectors_no - 1000)
			dvd_read_sector_streaming (d -> dvd, sector_no - 16 * 5 * 2, NULL, NULL, 0);
		else
			dvd_read_sector_streaming (d -> dvd, sector_no + 16 * 5, NULL, NULL, 0);
		if ((ret = dvd_read_sector_streaming (d -> dvd, sector_no, NULL, readbuf, BLOCK_SIZE)) >= 0) {
			for (j = 0; j < 5 && sector_no + j * 16 < d -> sectors_no && out; j++) {
				/* Reconstruct raw sectors */
				for (k = 0; k < 16; k++) {
					sect = &buf[j][k * RAW_SECTOR_SIZE];
					ram_offset = (j * RAW_BLOCK_SIZE) + k * RAW_SECTOR_SIZE;
					/* Get first 12 bytes (ID. IED and CPR_MAI fields) and last 4 bytes (EDC field) with memdump */
					if (j == 0 && k == 0) {
						if (dvd_memdump (d -> dvd, ram_offset, 1, 12, sect) < 0) {
							error ("Memdump (1) failed");
							out = false;
							retry = MAX_READ_RETRIES;		/* Well, if this fails going on is useless */
						}
					} else {
						memcpy (sect, tmp + 4, 12);
					}

					if (out && dvd_memdump (d -> dvd, ram_offset + 2060, 1, 16, tmp) < 0) {	/* Dumping in a single block is faster */
						error ("Memdump (2) failed");
						out = false;
					} else {
						memcpy (sect + 2060, tmp, 4);
					}
				}
			}

			/* Now the same for remaining 4 16-sector blocks */
			for (j = 0; j < 5 && sector_no + j * 16 < d -> sectors_no && out; j++) {
				if (j == 0 || (ret = dvd_read_sector_streaming (d -> dvd, sector_no + j * 16, NULL, readbuf, BLOCK_SIZE)) >= 0) {
					/* Copy "user data" field which has been incorrectly unscrambled by the DVD drive firmware */
					for (k = 0; k < 16; k++) {
						sect = &buf[j][k * RAW_SECTOR_SIZE];
						memcpy (sect + 12, readbuf + k * SECTOR_SIZE, SECTOR_SIZE);
					}
#ifdef DEBUG
					if (d -> unscrambling) {
#endif
						/* Try to unscramble all data to see if EDC fails */
						if (!unscrambler_unscramble_16sectors (d -> u, sector_no + (j * 16), buf[j], buf_unscrambled[j]))
							out = false;
#ifdef DEBUG
					}
#endif
				} else {
					error ("dvd_read_sector_streaming() failed with %d", ret);
					out = false;
				}
			}

			if (out) {
				/* It seems all data were unscrambled correctly, so cache them out */
				for (j = 0; j < 5 && sector_no + j * SECTORS_PER_BLOCK < d -> sectors_no; j++)
					disc_cache_add_block (d, start_block + j, buf_unscrambled[j], buf[j]);
			}
		} else {
			error ("dvd_read_sector_streaming() failed with %d", ret);
			out = false;
		}
	}

	if (!out)
		error ("Too many retries, giving up");

	return (out);
}


/* GCC-4160N / GCC-4240N (MN103S, DIC の Type1 相当) 用。
 * 16セクタを streaming READ でキャッシュし、0xA13000 から 1 ブロック分 (33024B) を
 * E7 で取り出す。DIC と同じシーケンスだが、余計なログ/照合を省くことで高速化する。 */
int disc_read_sector_10 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	bool out;
	u_int32_t start_block;
	int ret, retry;
	u_int8_t buf[16 * 2064];
	u_int8_t buf_unscrambled[16 * 2048];

	start_block = sector_no / SECTORS_PER_BLOCK;

	out = false;
	/* DIC と同様に、ひとつ前のブロックを READ してキャッシュを更新させると復帰しやすい */
	for (retry = 0; !out && retry < 5; retry++) {
		/* Assume everything will turn out well */
		out = true;

		if (retry > 0) {
			warning ("Read retry %d for sector %u", retry, sector_no);

			/* 直前ブロックを READ してドライブのキャッシュを更新させる (DIC の復帰手順) */
			if (start_block > 0)
				dvd_read_sector_streaming (d -> dvd, (start_block - 1) * SECTORS_PER_BLOCK, NULL, NULL, 0);
		}

		if ((ret = dvd_read_sector_streaming (d -> dvd, start_block * SECTORS_PER_BLOCK, NULL, NULL, 0)) < 0) {
			error ("dvd_read_sector_streaming() failed with %d", ret);
			out = false;
			continue;
		}

		/* 0xA13000 から 1 ブロック分の生セクタをまとめて取得 */
		if (dvd_memdump (d -> dvd, 0, 1, 16 * 2064, buf) < 0) {
			error ("Memdump failed");
			out = false;
			retry = MAX_READ_RETRIES;		/* Well, if this fails going on is useless */
			continue;
		}

		/* 生セクタ先頭のセクター番号が要求と一致するか確認（デシンク検出） */
		{
			u_int32_t sn = ((u_int32_t) buf[1] << 16) | ((u_int32_t) buf[2] << 8) | buf[3];
			u_int32_t exp = start_block * SECTORS_PER_BLOCK + 0x30000;
			if (sn != exp && retry == 0)
				warning ("Sector num from cache: got 0x%X, expected 0x%X (block %u)", sn, exp, start_block);
		}

#ifdef DEBUG
		if (d -> unscrambling) {
#endif
			/* Try to unscramble all data to see if EDC fails */
			if (!unscrambler_unscramble_16sectors (d -> u, start_block * SECTORS_PER_BLOCK, buf, buf_unscrambled))
				out = false;
#ifdef DEBUG
		}
#endif

		if (out)
			disc_cache_add_block (d, start_block, buf_unscrambled, buf);
	}

	if (!out)
		error ("Too many retries, giving up");

	return (out);
}


/* ---------- method13: Hitachi Type2 (GCC-4241N / GCC-4242N) ----------
 * DIC の 0xe7 Type2_1/2_2 相当。baseAddr≒0x80000000 を 0x2040 (= 4セクタ × 2064B)
 * ずつ回転させ、4セクタ単位の E7 で 1 ブロックを読む方式。
 *
 * TODO(次回): 実機（GCC-4241N）でキャッシュ配置・回転量・4セクタE7 の挙動を実測して
 * 実装する（tools/probe/ に配置実測プローブを追加）。現状は未実装のため、誤った
 * データを返さないよう明示的に失敗する。 */
int disc_read_sector_13 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata) {
	(void) d;
	(void) sector_no;
	(void) data;
	(void) rawdata;
	error ("method13 (Hitachi Type2 / GCC-4241N・4242N) は未実装です。"
		"DIC の Type2 配置（0x80000000 回転ベース・4セクタE7）を実測後に実装してください");
	return (false);
}
