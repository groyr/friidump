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
 * \brief disc 実装の内部共有定義（disc.c / disc_fast.c / disc_hitachi.c 専用）。
 *
 * disc 構造体は本来ライブラリ外に公開しないが、読み出し方式ごとに実装ファイルを
 * 分割するため、実装ファイル間でのみ共有する。
 */

#ifndef DISC_INTERNAL_H_INCLUDED
#define DISC_INTERNAL_H_INCLUDED

#include "constants.h"
#include "disc.h"
#include "dvd_drive.h"
#include "unscrambler.h"

/*! \brief 読み出しリトライ回数（method7-10 等が使用）。 */
#define MAX_READ_RETRIES 5


typedef int (*disc_read_sector_func) (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);


/*! \brief A structure that represents a Nintendo GameCube/Wii optical disc.
 */
struct disc_s {
	dvd_drive *dvd;				//!< The structure for the DVD-drive the disc is inserted in.
	disc_type type;				//!< The disc type.
	char system_id;				//!< A letter identifying the target system.
	char game_id[2 + 1];			//!< Two letters identifying the game.
	disc_region region;			//!< The disc region.
	char maker[3];				//!< Two letters identifying the maker of the game.
	u_int8_t version;			//!< A number identifying the game version.
	char *version_string;			//!< The same as <code>version</code>, in a more human-understandable format.
	char *title;				//!< The game title.
	bool has_update;			//!< True if the game contains a system update (Only possible for Wii discs).
	u_int32_t sectors_no;			//!< The number of sectors of the disc.
	u_int32_t layerbreak;			//!< For dual-layer DVDs.

	u_int32_t sec_disc;
	u_int32_t sec_mem;
	u_int32_t max_cnt;
	u_int32_t max_blk;

	/* Read function & stuff */
	int command;				//!< Buffer access command ID.
	int read_method;			//!< The read method ID.
	disc_read_sector_func read_sector;	//!< The actual function that will be used to perform read operations, corresponding to <code>read_method</code>.
	bool unscrambling;			//!< If true, raw data read from the disc will be unscrambled to assure it is error-free. Disabling this is only useful for raw performance tests.
	unscrambler *u;				//!< The unscrambler structure that will be used to perform the unscrambling.

	/* Read cache */
	u_int32_t cache_size;			//!< The number of blocks that will be cached when read.
	u_int8_t **raw_cache;			//!< Memory area for raw sectors cache.
	u_int8_t **cache;			//!< Memory area for unscrambled sectors cache.
	u_int32_t *cache_map;			//!< Data structure used by the caching system to know which blocks are in memory.

	/* fast方式 (method11) 用の補正テーブル。
	 * corr[m][i] = cipher(drive_seed_m)[i] XOR cipher(gc_seed_m)[i]
	 * 位相 m はブロック番号 mod 16。 */
	bool fast_ready;
	u_int8_t fast_corr[16][2042];

	/* method12(rawマスター+EDC) 用。
	 * drive_cipher[m][i] = raw[12+i] XOR host[i]（ドライブ逆スクランブル鍵そのもの）。
	 * これで host データから生フレーム raw[12:2060] を復元できる。 */
	bool fast12_ready;
	u_int8_t drive_cipher12[16][2048];

	/* 二層ディスク(Wii DL)対応。
	 * 第2層は READ(12) に渡す LBA と物理セクタ番号(sn)の関係が第1層と異なる。
	 * 実測: 第1層 sn = LBA + 0x30000、第2層 sn = LBA + layer_sn_offset2。
	 * layer_sn_offset2 は第2層先頭ブロックの sn から算出する（0 なら未確定）。 */
	u_int32_t layer_sn_offset2;
	bool layer2_ready;
	/* 第2層用の校正テーブル（第1層と層が異なるため別に持つ）。 */
	bool fast12_ready2;
	u_int8_t drive_cipher12_2[16][2048];
	/* 第2層用の method11 補正テーブル（corr[m][i] = rd[i] XOR P[6+i]）。 */
	bool fast_ready2;
	u_int8_t fast_corr2[16][2042];
};


/* disc.c が提供する共有ヘルパー */
void disc_cache_add_block (disc *d, u_int32_t block, u_int8_t *data, u_int8_t *rawdata);
void disc_cache_add_block_raw (disc *d, u_int32_t block, u_int8_t *data, u_int8_t *rawtrue);
bool disc_cache_lookup_block (disc *d, u_int32_t block, u_int8_t **data, u_int8_t **rawdata);

/* 読み出し方式（disc_hitachi.c / disc_fast.c で実装） */
int disc_read_sector_7 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);
int disc_read_sector_8 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);
int disc_read_sector_9 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);
int disc_read_sector_10 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);
int disc_read_sector_11 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);
int disc_read_sector_12 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);
int disc_read_sector_13 (disc *d, u_int32_t sector_no, u_int8_t **data, u_int8_t **rawdata);

#endif
