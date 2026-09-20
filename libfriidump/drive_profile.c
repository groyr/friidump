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
 * \brief ドライブ特性テーブル（vendor / prod_id → memdump / E7 ベース / 読み出し方式）。
 */

#include <string.h>
#include "drive_profile.h"

/* Imported drive-specific memory dump functions */
int vanilla_2064_dvd_dump_mem	(dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);
int vanilla_2384_dvd_dump_mem	(dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);
int hitachi_dvd_dump_mem	(dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);
int hitachi_mn103s_dump_mem	(dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);
int liteon_dvd_dump_mem		(dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);
int renesas_dvd_dump_mem	(dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);


/*! \brief ドライブ特性テーブル（上から順に評価する）。
 *
 * - Type1 (GCC-4160N/4240N): データフレームは 0xA13000。fast方式(method12)が既定。
 * - Type2 (GCC-4241N/4242N): 0x80000000 回転ベース・4セクタE7。method13 は未実装スタブ。
 * - Type4 (その他 MN103 系): 0x80000000 固定ベース。method9。
 *
 * INQUIRY の製品IDは "DVD-ROM GDR8082N" や "RW/DVD GCC-4240N" のように
 * 接頭辞・スラッシュ表記が機種ごとに異なるため、部分一致で判定する。
 */
static const drive_profile PROFILES[] = {
	/* --- Hitachi MN103S Type1 (0xA13000, fast method12) --- */
	{ "HL-DT-ST", "GCC-4160N", hitachi_mn103s_dump_mem, 0xA13000, READ_FAMILY_HITACHI_TYPE1, 12, 2 },
	{ "HL-DT-ST", "GCC-4240N", hitachi_mn103s_dump_mem, 0xA13000, READ_FAMILY_HITACHI_TYPE1, 12, 2 },
	/* --- Hitachi MN103S Type2 (0x80000000 rotating base, 4-sector E7; method13 stub) --- */
	{ "HL-DT-ST", "GCC-4241N", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE2, 13, 2 },
	{ "HL-DT-ST", "GCC-4242N", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE2, 13, 2 },
	/* --- Hitachi MN103 (0x80000000 fixed base, method9) --- */
	{ "HL-DT-ST", "GDR8082N", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	{ "HL-DT-ST", "GDR8161B", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	{ "HL-DT-ST", "GDR8162B", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	{ "HL-DT-ST", "GDR8163B", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	{ "HL-DT-ST", "GDR8164B", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	{ "HL-DT-ST", "GCC-4243N", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	{ "HL-DT-ST", "GCC-4244N", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	{ "HL-DT-ST", "GCC-4247N", hitachi_dvd_dump_mem, 0x80000000, READ_FAMILY_HITACHI_TYPE4, 9, 2 },
	/* --- Lite-On --- */
	{ "LITE-ON", "DVDRW LH-18A1H", liteon_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 5, 3 },
	{ "LITE-ON", "DVDRW LH-18A1P", liteon_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 5, 3 },
	{ "LITE-ON", "DVDRW LH-20A1H", liteon_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 5, 3 },
	{ "LITE-ON", "DVDRW LH-20A1P", liteon_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 5, 3 },
	/* --- Toshiba Samsung --- */
	{ "TSSTcorp", "DVD-ROM SH-D162A", vanilla_2384_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 0, 1 },
	{ "TSSTcorp", "DVD-ROM SH-D162B", vanilla_2384_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 0, 1 },
	{ "TSSTcorp", "DVD-ROM SH-D162C", vanilla_2384_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 0, 1 },
	{ "TSSTcorp", "DVD-ROM SH-D162D", vanilla_2384_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 0, 1 },
	/* --- Plextor (any product) --- */
	{ "PLEXTOR", NULL, vanilla_2064_dvd_dump_mem, 0, READ_FAMILY_DEFAULT, 2, 0 },
};


const drive_profile *drive_profile_lookup (const char *vendor, const char *prod_id) {
	size_t i;

	for (i = 0; i < sizeof (PROFILES) / sizeof (PROFILES[0]); i++) {
		const drive_profile *p = &PROFILES[i];
		if (strcmp (vendor, p -> vendor) != 0)
			continue;
		if (p -> prod_substr && strstr (prod_id, p -> prod_substr) == NULL)
			continue;
		return p;
	}
	return NULL;
}
