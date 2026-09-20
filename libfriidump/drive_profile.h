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

#ifndef DRIVE_PROFILE_H_INCLUDED
#define DRIVE_PROFILE_H_INCLUDED

#include "dvd_drive.h"
#include <sys/types.h>

/*! \brief ドライブ特性の定義。
 *
 * vendor / prod_id から memdump 実装・E7 ベース・読み出しファミリ・既定 method・
 * コマンド ID を引く。dvd_assign_functions() がこのテーブルを参照することで、
 * 機種の追加・変更がデータ1行で済む。
 */
typedef struct {
	const char *vendor;			//!< ENQUIRY の vendor（完全一致）
	const char *prod_substr;		//!< prod_id に含まれる文字列（NULL なら vendor のみで一致）
	dvd_drive_memdump_func memdump;		//!< E7 メモリダンプ実装
	u_int32_t mem_base;			//!< E7 のベースアドレス（情報用）
	read_family family;			//!< 読み出し方式ファミリ
	int def_method;				//!< 既定の読み出し method
	u_int32_t command;			//!< memdump コマンド ID（0=vanilla2064, 1=vanilla2384, 2=hitachi, 3=liteon, 4=renesas）
} drive_profile;

/*! \brief vendor / prod_id からドライブ特性を引く（見つからなければ NULL）。 */
const drive_profile *drive_profile_lookup (const char *vendor, const char *prod_id);

#endif
