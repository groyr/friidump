/***************************************************************************
 *   Copyright (C) 2007 by Arep                                            *
 *   Support is provided through the forums at                             *
 *   http://www.console-tribe.com                                          *
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

#include "misc.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <multihash.h>
#include "constants.h"
#include "disc.h"
#include "dumper.h"

#ifndef WIN32
#include <unistd.h>
#include <sys/types.h>
#include <fcntl.h>
#else
#include <io.h>
#endif

/*! \brief ジャーナルを書き込む間隔（セクタ単位）。
 * EDC 検証済みの完了位置を記録する。以前は 320（20ブロック）ごとに fsync して
 * いたが、fsync が吸い出しループを止め SD の書き込みスパイクを招くため、
 * クラッシュ時の巻き戻りを許容して 8192（512ブロック ≒16MB）ごとに緩和した。 */
#define JOURNAL_INTERVAL 8192

/*! \brief 出力（raw/iso）の stdio バッファ長。
 * 毎セクタ fflush すると 1 セクタごとに write syscall が発生するため、
 * 1MB バッファにまとめてから書き出す。 */
#define OUTPUT_BUFFER_SIZE (1024 * 1024)

struct dumper_s {
	disc *dsk;
	char *outfile_raw;
	u_int32_t start_sector_raw;
	FILE *fp_raw;
	char *outfile_iso;
	u_int32_t start_sector_iso;
	FILE *fp_iso;
	u_int32_t start_sector;
	bool hashing;
	bool flushing;
	bool resume;

	/*! 再開用ジャーナル（クラッシュ時のテール破損対策）。
	 * EDC検証済みで書き込みが完了した最後のセクタを記録する。 */
	char *journal_path;
	FILE *fp_journal;

	multihash hash_raw;
	multihash hash_iso;
	
	progress_func progress;
	void *progress_data;
};


/*! \brief 出力ファイルの領域を事前確保する（microSD の断片化・遅延回避）。
 * 失敗しても致命的ではないため、戻り値は見ないで呼んでよい。 */
static void dumper_preallocate (FILE *fp, int64_t size) {
#ifdef WIN32
	(void) fp;
	(void) size;
#else
	int fd;
	if (!fp || size <= 0)
		return;
	fd = fileno (fp);
	if (fd < 0)
		return;
	if (posix_fallocate (fd, 0, (off_t) size) != 0) {
		/* 空き不足などで失敗しても、通常の書き込みでは問題ない */
	}
#endif
}


/*! \brief ジャーナルをディスクへ同期する（クラッシュ耐性）。 */
static void dumper_journal_sync (FILE *fp) {
#ifdef WIN32
	if (fp) _commit (_fileno (fp));
#else
	if (fp) fsync (fileno (fp));
#endif
}


/*! \brief 直近の完了位置をジャーナルへ書き込む。
 * @param next_sector 次に読むべきセクタ（ブロック境界）。 */
static void dumper_journal_write (dumper *dmp, u_int32_t next_sector) {
	if (!dmp -> fp_journal)
		return;
	rewind (dmp -> fp_journal);
	fprintf (dmp -> fp_journal, "next=%u\n", next_sector);
	fflush (dmp -> fp_journal);
	dumper_journal_sync (dmp -> fp_journal);
}


/*! \brief ジャーナルから再開位置を読む。無ければ 0。 */
static u_int32_t dumper_journal_read (const char *path) {
	FILE *fp;
	char line[64];
	u_int32_t out;

	out = 0;
	fp = fopen (path, "rb");
	if (!fp)
		return (0);
	if (fgets (line, sizeof (line), fp)) {
		unsigned long v;
		if (sscanf (line, "next=%lu", &v) == 1)
			out = (u_int32_t) v;
	}
	fclose (fp);
	return (out);
}


/**
 * Tries to open the output file for writing and to find out if it contains valid data so that the dump can continue.
 * @param dvd 
 * @param outfile 
 * @param[out] fp The file pointer to write to.
 * @param[out] start_sector The sector to start reading from.
 * @return true if the dumping can start/continue, false otherwise (for instance if outfile cannot be written to).
 */
bool dumper_set_raw_output_file (dumper *dmp, char *outfile_raw, bool resume) {
	bool out;
	my_off_t filesize;
	FILE *fp;

	dmp -> resume = dmp -> resume || resume;

	if (!outfile_raw) {
		/* Raw output disabled */
		out = true;
		dmp -> start_sector_raw = -1;
		my_free (dmp -> outfile_raw);
		dmp -> outfile_raw = NULL;
	} else if (dmp -> outfile_raw) {
		error ("Raw output file already defined");
		out = false;
	} else if (!(fp = fopen (outfile_raw, "rb"))) {		/** @todo Maybe we could not open file for permission problems */
		/* Raw output file does not exist, start from scratch */
		out = true;
		dmp -> start_sector_raw = 0;
		my_strdup (dmp -> outfile_raw, outfile_raw);
	} else if (resume) {
		/* Raw output file exists and resume was requested, so see how many dumped sectors it contains */
		my_fseek (fp, 0, SEEK_END);
		filesize = my_ftell (fp);
		fclose (fp);
		out = true;
		dmp -> start_sector_raw = (u_int32_t) (filesize / RAW_SECTOR_SIZE / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK;
		debug ("Raw output can restart from sector %u", dmp -> start_sector_raw);
		my_strdup (dmp -> outfile_raw, outfile_raw);
	} else {
		/* Raw output file exists but resume was not requested, error */
		fclose (fp);
		error ("Raw output file exists, but resume was not requested.");
		out = false;
		dmp -> start_sector_raw = -1;
		my_free (dmp -> outfile_raw);
		dmp -> outfile_raw = NULL;
	}

	return (out);
}


bool dumper_set_iso_output_file (dumper *dmp, char *outfile_iso, bool resume) {
	bool out;
	my_off_t filesize;
	FILE *fp;

	dmp -> resume = dmp -> resume || resume;

	if (!outfile_iso) {
		/* Raw output disabled */
		out = true;
		dmp -> start_sector_iso = -1;
		my_free (dmp -> outfile_iso);
		dmp -> outfile_iso = NULL;
	} else if (dmp -> outfile_iso) {
		error ("ISO output file already defined");
		out = false;
	} else if (!(fp = fopen (outfile_iso, "rb"))) {		/** @todo Maybe we could not open file for permission problems */
		/* Raw output file does not exist, start from scratch */
		out = true;
		dmp -> start_sector_iso = 0;
		my_strdup (dmp -> outfile_iso, outfile_iso);
	} else if (resume) {
		/* Raw output file exists and resume was requested, so see how many dumped sectors it contains */
		my_fseek (fp, 0, SEEK_END);
		filesize = my_ftell (fp);
		fclose (fp);
		out = true;
		dmp -> start_sector_iso = (u_int32_t) (filesize / SECTOR_SIZE / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK;
		debug ("ISO output can restart from sector %u", dmp -> start_sector_iso);
		my_strdup (dmp -> outfile_iso, outfile_iso);
	} else {
		/* Raw output file exists but resume was not requested, error */
		fclose (fp);
		error ("ISO output file exists, but resume was not requested.");
		out = false;
		dmp -> start_sector_iso = -1;
		my_free (dmp -> outfile_iso);
		dmp -> outfile_iso = NULL;
	}

	return (out);
}


bool dumper_prepare (dumper *dmp) {
	bool out;
	u_int8_t buf[RAW_SECTOR_SIZE];
	size_t r;
	u_int32_t i;

	/* Outputting to both files, resume must start from the file with the least sectors. Hopefully they will have the same number of sectors, anyway... */
	if (dmp -> outfile_raw && dmp -> outfile_iso && dmp -> start_sector_raw != dmp -> start_sector_iso) {
		if (dmp -> start_sector_raw < dmp -> start_sector_iso)
			dmp -> start_sector = dmp -> start_sector_raw;
		else
			dmp -> start_sector = dmp -> start_sector_iso;
	} else if (dmp -> outfile_raw) {
		dmp -> start_sector = dmp -> start_sector_raw;
	} else if (dmp -> outfile_iso) {
		dmp -> start_sector = dmp -> start_sector_iso;
	} else {
		MY_ASSERT (0);
	}

	/* ジャーナル: raw 優先でパスを決め、resume時は未フラッシュ分を切り捨てる */
	{
		const char *base = dmp -> outfile_raw ? dmp -> outfile_raw : dmp -> outfile_iso;
		if (base) {
			size_t l = strlen (base) + 9;
			dmp -> journal_path = (char *) malloc (l);
			snprintf (dmp -> journal_path, l, "%s.journal", base);
		}
		if (dmp -> resume && dmp -> journal_path) {
			u_int32_t j = dumper_journal_read (dmp -> journal_path);
			if (j > 0 && j < dmp -> start_sector) {
				dmp -> start_sector = (j / SECTORS_PER_BLOCK) * SECTORS_PER_BLOCK;
				debug ("Journal: resuming from sector %u", dmp -> start_sector);
			}
		}
	}

	/* Prepare hashes */
	if (dmp -> hashing) {
		multihash_init (&(dmp -> hash_raw));
		multihash_init (&(dmp -> hash_iso));
	}

	/* Setup raw output file */
	if (dmp -> outfile_raw) {
		dmp -> fp_raw = fopen (dmp -> outfile_raw, "a+b");
		if (dmp -> fp_raw)
			setvbuf (dmp -> fp_raw, NULL, _IOFBF, OUTPUT_BUFFER_SIZE);

		if (dmp -> hashing) {
			debug ("Calculating hashes for pre-existing raw dump data");
			if (dmp -> start_sector > 0) {
				for (i = 0; i < dmp -> start_sector && (r = fread (buf, RAW_SECTOR_SIZE, 1, dmp -> fp_raw)) > 0; i++)
					multihash_update (&(dmp -> hash_raw), buf, RAW_SECTOR_SIZE);
				MY_ASSERT (r > 0);
			}
		}

		/* Now call fseek as file will only be written, from now on */
		if (my_fseek (dmp -> fp_raw, dmp -> start_sector * RAW_SECTOR_SIZE, SEEK_SET) == 0 &&
		    ftruncate (fileno (dmp -> fp_raw), (int64_t) dmp -> start_sector * RAW_SECTOR_SIZE) == 0) {
			dumper_preallocate (dmp -> fp_raw, (int64_t) disc_get_sectors_no (dmp -> dsk) * RAW_SECTOR_SIZE);
			out = true;
			debug ("Writing to file \"%s\" in raw format (fseeked() to %lld)", dmp -> outfile_raw, my_ftell (dmp -> fp_raw));
		} else {
			out = false;
			fclose (dmp -> fp_raw);
			dmp -> fp_raw = NULL;
		}
	} else {
		dmp -> fp_raw = NULL;
	}

	/* Setup ISO output file */
	if (dmp -> outfile_iso) {
		dmp -> fp_iso = fopen (dmp -> outfile_iso, "a+b");
		if (dmp -> fp_iso)
			setvbuf (dmp -> fp_iso, NULL, _IOFBF, OUTPUT_BUFFER_SIZE);

		if (dmp -> hashing) {
			debug ("Calculating hashes for pre-existing ISO dump data");
			if (dmp -> start_sector > 0) {
				for (i = 0; i < dmp -> start_sector && (r = fread (buf, SECTOR_SIZE, 1, dmp -> fp_iso)) > 0; i++)
					multihash_update (&(dmp -> hash_iso), buf, SECTOR_SIZE);
				MY_ASSERT (r > 0);
			}
		}

		if (my_fseek (dmp -> fp_iso, dmp -> start_sector * SECTOR_SIZE, SEEK_SET) == 0 &&
		    ftruncate (fileno (dmp -> fp_iso), (int64_t) dmp -> start_sector * SECTOR_SIZE) == 0) {
			dumper_preallocate (dmp -> fp_iso, (int64_t) disc_get_sectors_no (dmp -> dsk) * SECTOR_SIZE);
			out = true;
			debug ("Writing to file \"%s\" in ISO format (fseeked() to %lld)", dmp -> outfile_iso, my_ftell (dmp -> fp_iso));
		} else {
			out = false;
			fclose (dmp -> fp_iso);
			dmp -> fp_iso = NULL;
		}
	} else {
		dmp -> fp_iso = NULL;
	}

	/* ジャーナルを書き込み用に開き、開始位置を記録 */
	if (out && dmp -> journal_path) {
		dmp -> fp_journal = fopen (dmp -> journal_path, "wb");
		if (dmp -> fp_journal)
			dumper_journal_write (dmp, dmp -> start_sector);
	}

	return (out);
}


int dumper_dump (dumper *dmp, u_int32_t *current_sector) {
	bool out;
	u_int8_t *rawbuf, *isobuf;
	u_int32_t i, sectors_no, last_sector;
#ifdef DEBUGaa
	bool no_unscrambling;
#endif

#ifdef DEBUGaa
	no_unscrambling = dd -> no_unscrambling;
	if (fp_iso && no_unscrambling) {
		warning ("Output to ISO format requested, ignoring no_unscrambling!");
		no_unscrambling = false;
	}
#endif

	sectors_no = disc_get_sectors_no (dmp -> dsk);
#if 0
	if (dd -> start_sector != -1) {
		if (dd -> start_sector >= sectors_no) {
			error ("Cannot start dumping from sector %u as the inserted disc only has %u sectors\n", dd -> start_sector, sectors_no);
			out = false;
		} else {
			warning ("Start sector forced to %u\n", dd -> start_sector);
			ss = dd -> start_sector;

			if (fp_iso)
				fseek (fp_iso, ss * 2048, SEEK_SET);
			if (fp_raw)
				fseek (fp_raw, ss * 2064, SEEK_SET);
		}
	}
#endif
	
	if (true) {
		debug ("Starting dump process from sector %u...\n", dmp -> start_sector);

		/* First call to progress function */
		if (dmp -> progress)
			dmp -> progress (true, dmp -> start_sector, sectors_no, dmp -> progress_data);

		last_sector=sectors_no-1;
		
		for (i = dmp -> start_sector, out = true; i < sectors_no && out; i++) {
			disc_read_sector (dmp -> dsk, i, &isobuf, &rawbuf);

			if (dmp -> fp_raw) {
				clearerr (dmp -> fp_raw);

				if (!rawbuf) {
					error ("NULL buffer");
					out = false;
					*(current_sector) = i;
				}
				else fwrite (rawbuf, RAW_SECTOR_SIZE, 1, dmp -> fp_raw);
				if (ferror (dmp -> fp_raw)) {
					error ("fwrite() to raw output file failed");
					out = false;
					*(current_sector) = i;
				}

				/* fflush はループ末尾でまとめて行う（毎セクタ write を避ける） */

				if (dmp -> hashing && out)
					multihash_update (&(dmp -> hash_raw), rawbuf, RAW_SECTOR_SIZE);
			}

			if (dmp -> fp_iso) {
				clearerr (dmp -> fp_iso);

				if (!isobuf) {
					error ("NULL buffer");
					out = false;
					*(current_sector) = i;
				}
				else fwrite (isobuf, SECTOR_SIZE, 1, dmp -> fp_iso);

				if (ferror (dmp -> fp_iso)) {
					error ("fwrite() to ISO output file failed");
					out = false;
					*(current_sector) = i;
				}

				/* fflush はループ末尾でまとめて行う（毎セクタ write を避ける） */

				if (dmp -> hashing && out)
					multihash_update (&(dmp -> hash_iso), isobuf, SECTOR_SIZE);
			}

			/* 進捗更新の区切り（320 セクタ≒640KB）でだけ出力を flush する。
			 * 以前は毎セクタ fflush していたが、write syscall を削減するため
			 * 進捗・ジャーナルと同じ粗い粒度に揃えた。 */
			if ((i % 320 == 0) || (i == last_sector)) { //speedhack
				if (dmp -> flushing) {
					if (dmp -> fp_raw)
						fflush (dmp -> fp_raw);
					if (dmp -> fp_iso)
						fflush (dmp -> fp_iso);
				}
				if (dmp -> progress)
					dmp -> progress (false, i + 1, sectors_no, dmp -> progress_data);
			}

			/* EDC検証済みの完了位置をジャーナルへ記録（クラッシュ耐性） */
			if (dmp -> fp_journal && (((i + 1) % JOURNAL_INTERVAL) == 0 || i == last_sector))
				dumper_journal_write (dmp, i + 1);
		}

		if (dmp -> hashing) {
			multihash_finish (&(dmp -> hash_raw));
			multihash_finish (&(dmp -> hash_iso));
		}

		if (dmp -> fp_raw)
			fclose (dmp -> fp_raw);
		if (dmp -> fp_iso)
			fclose (dmp -> fp_iso);
		if (out) {


		}
	}

	return (out);
}


dumper *dumper_new (disc *d) {
	dumper *dmp;

	dmp = (dumper *) malloc (sizeof (dumper));
	memset (dmp, 0, sizeof (dumper));
	dmp -> dsk = d;
	dumper_set_hashing (dmp, true);
	dumper_set_flushing (dmp, true);

	return (dmp);
}


void dumper_set_progress_callback (dumper *dmp, progress_func progress, void *progress_data) {
	dmp -> progress = progress;
	dmp -> progress_data = progress_data;

	return;
}


void dumper_set_hashing (dumper *dmp, bool h) {
	dmp -> hashing = h;
	debug ("Hashing %s", h ? "enabled" : "disabled");

	return;
}


void dumper_set_flushing (dumper *dmp, bool f) {
	dmp -> flushing = f;
	debug ("Flushing %s", f ? "enabled" : "disabled");

	return;
}

void *dumper_destroy (dumper *dmp) {
	my_free (dmp -> outfile_raw);
	my_free (dmp -> outfile_iso);
	if (dmp -> fp_journal)
		fclose (dmp -> fp_journal);
	my_free (dmp -> journal_path);
	my_free (dmp);

	return (NULL);
}


char *dumper_get_iso_crc32 (dumper *dmp) {
	return ((dmp -> hash_iso).crc32_s);
}


char *dumper_get_raw_crc32 (dumper *dmp) {
	return ((dmp -> hash_raw).crc32_s);
}


char *dumper_get_iso_md4 (dumper *dmp) {
	return ((dmp -> hash_iso).md4_s);
}


char *dumper_get_raw_md4 (dumper *dmp) {
	return ((dmp -> hash_raw).md4_s);
}


char *dumper_get_iso_md5 (dumper *dmp) {
	return ((dmp -> hash_iso).md5_s);
}


char *dumper_get_raw_md5 (dumper *dmp) {
	return ((dmp -> hash_raw).md5_s);
}


char *dumper_get_iso_ed2k (dumper *dmp) {
	return ((dmp -> hash_iso).ed2k_s);
}


char *dumper_get_raw_ed2k (dumper *dmp) {
	return ((dmp -> hash_raw).ed2k_s);
}


char *dumper_get_iso_sha1 (dumper *dmp) {
	return ((dmp -> hash_iso).sha1_s);
}


char *dumper_get_raw_sha1 (dumper *dmp) {
	return ((dmp -> hash_raw).sha1_s);
}
