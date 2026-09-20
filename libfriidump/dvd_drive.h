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

#ifndef DVD_DRIVE_H_INCLUDED
#define DVD_DRIVE_H_INCLUDED

#include "misc.h"
#include <sys/types.h>


typedef struct dvd_drive_s dvd_drive;


/*! \brief ドライブの読み出し方式ファミリ（E7 のキャッシュ配置・ベース方式）。
 *
 * drive_profile テーブルで機種ごとに割り当てる。読み出し関数の選択や、
 * 将来の Type2 対応の切り分けに使う。 */
typedef enum {
	READ_FAMILY_DEFAULT = 0,	//!< 汎用（memdump 実装依存）
	READ_FAMILY_HITACHI_TYPE1,	//!< 0xA13000 固定ベース（GCC-4160N/4240N）
	READ_FAMILY_HITACHI_TYPE2,	//!< 0x80000000 回転ベース・4セクタE7（GCC-4241N/4242N）
	READ_FAMILY_HITACHI_TYPE4	//!< 0x80000000 固定ベース（従来の MN103 系）
} read_family;



typedef struct {
	int sense_key;
	int asc;
	int ascq;
} req_sense;


typedef struct {
	u_int8_t cmd[12];
	req_sense *sense;
	u_int8_t *buffer;
	int buflen;
} mmc_command;


/* Functions */
dvd_drive *dvd_drive_new (char *device, u_int32_t command);
void *dvd_drive_destroy (dvd_drive *d);
int dvd_read_sector_dummy (dvd_drive *dvd, u_int32_t sector, u_int32_t sectors, req_sense *sense, u_int8_t *extbuf, size_t extbufsize);
int dvd_read_sector_streaming (dvd_drive *dvd, u_int32_t sector, req_sense *sense, u_int8_t *extbuf, size_t extbufsize);
int dvd_read_streaming (dvd_drive *dvd, u_int32_t sector, u_int32_t sectors, req_sense *sense, u_int8_t *extbuf, size_t extbufsize);
int dvd_flush_cache_READ12 (dvd_drive *dvd, u_int32_t sector, req_sense *sense);
int dvd_stop_unit (dvd_drive *dvd, bool start, req_sense *sense);
int dvd_set_speed (dvd_drive *dvd, u_int32_t speed, req_sense *sense);
int dvd_get_size (dvd_drive *dvd, u_int32_t *size, req_sense *sense);
int dvd_get_layerbreak (dvd_drive *dvd, u_int32_t *layerbreak, req_sense *sense);
int dvd_set_streaming (dvd_drive *dvd, u_int32_t speed, req_sense *sense);
int dvd_memdump (dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);
char *dvd_get_vendor (dvd_drive *dvd);
char *dvd_get_product_id (dvd_drive *dvd);
char *dvd_get_product_revision (dvd_drive *dvd);
char *dvd_get_model_string (dvd_drive *dvd);
bool dvd_get_support_status (dvd_drive *dvd);
u_int32_t dvd_get_def_method (dvd_drive *dvd);
u_int32_t dvd_get_command (dvd_drive *dvd);
u_int32_t dvd_get_mem_base (dvd_drive *dvd);
read_family dvd_get_read_family (dvd_drive *dvd);

/* The following are exported for use by drive-specific functions */
typedef int (*dvd_drive_memdump_func) (dvd_drive *dvd, u_int32_t block_off, u_int32_t block_len, u_int32_t block_size, u_int8_t *buf);
void dvd_init_command (mmc_command *mmc, u_int8_t *buf, int len, req_sense *sense);
int dvd_execute_cmd (dvd_drive *dvd, mmc_command *mmc, bool ignore_errors);

#endif
