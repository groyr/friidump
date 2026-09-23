/***************************************************************************
 *  C 実装から Rust 移植のゴールデンベクタを生成する使い捨てハーネス。
 *
 *  ecma-267.c / unscrambler.c をそのままリンクし、EDC・LFSR・スクランブル解除の
 *  出力をテキストで書き出す。Rust 側の統合テスト (tests/golden.rs) が読み込んで
 *  突き合わせる。
 *
 *  ビルド例 (MSYS2 UCRT64):
 *    gcc -O2 -Ilibfriidump rust/tools/gen_vectors.c \
 *        libfriidump/ecma-267.c libfriidump/unscrambler.c -o gen_vectors.exe
 *
 *  実行: gen_vectors.exe <出力ディレクトリ>
 ***************************************************************************/

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "misc.h"
#include "constants.h"
#include "ecma-267.h"
#include "unscrambler.h"

#define EDC_LENGTH (RAW_SECTOR_SIZE - 4)

/* unscrambler.c が参照する large-file 用ヘルパのスタブ。
 * 本ハーネスはファイル I/O を使わないため、misc.c をリンクせずここで定義する。 */
int my_fseek(FILE *fp, my_off_t offset, int whence) {
	return _fseeki64(fp, (long long) offset, whence);
}
my_off_t my_ftell(FILE *fp) {
	return (my_off_t) _ftelli64(fp);
}

/* バイト列を 16 進文字列で出力する */
static void to_hex(FILE *f, const unsigned char *p, size_t n) {
	size_t i;
	for (i = 0; i < n; i++)
		fprintf(f, "%02x", p[i]);
}

/* ISO ブロックを指定 seed でスクランブルし、正しい EDC 付き raw ブロックを作る。
 * unscramble_frame() の逆手順。 */
static void make_raw(unsigned char *raw, const unsigned char *iso,
                     unsigned short seed, unsigned int first_sector) {
	unsigned char cipher[SECTOR_SIZE];
	int j, i;

	LFSR_init(seed);
	for (i = 0; i < SECTOR_SIZE; i++)
		cipher[i] = LFSR_byte();

	for (j = 0; j < SECTORS_PER_BLOCK; j++) {
		unsigned char *frame = raw + RAW_SECTOR_SIZE * j;
		const unsigned char *is = iso + SECTOR_SIZE * j;
		unsigned int sn = first_sector + (unsigned int) j;
		unsigned char tmp[RAW_SECTOR_SIZE];
		unsigned int edc;

		frame[0] = 0;
		frame[1] = (unsigned char) ((sn >> 16) & 0xff);
		frame[2] = (unsigned char) ((sn >> 8) & 0xff);
		frame[3] = (unsigned char) (sn & 0xff);
		frame[4] = 0;
		frame[5] = 0;
		memcpy(frame + 6, is, 6);
		for (i = 0; i < SECTOR_SIZE - 6; i++)
			frame[12 + i] = (unsigned char) (is[6 + i] ^ cipher[i]);
		for (i = 0; i < 6; i++)
			frame[2054 + i] = 0;

		memcpy(tmp, frame, RAW_SECTOR_SIZE);
		LFSR_init(seed);
		for (i = 12; i < EDC_LENGTH; i++)
			tmp[i] ^= LFSR_byte();
		edc = edc_calc(0, tmp, EDC_LENGTH);
		frame[2060] = (unsigned char) ((edc >> 24) & 0xff);
		frame[2061] = (unsigned char) ((edc >> 16) & 0xff);
		frame[2062] = (unsigned char) ((edc >> 8) & 0xff);
		frame[2063] = (unsigned char) (edc & 0xff);
	}
}

static void emit_edc(FILE *f, const unsigned char *data, size_t len) {
	unsigned int edc = edc_calc(0, (u8 *) data, (u32) len);
	fprintf(f, "%zu ", len);
	to_hex(f, data, len);
	fprintf(f, " %08x\n", edc);
}

int main(int argc, char **argv) {
	const char *dir = (argc > 1) ? argv[1] : ".";
	char path[1024];
	FILE *f;
	unsigned char buf[BLOCK_SIZE];
	unsigned char raw[RAW_BLOCK_SIZE];
	unsigned char out[BLOCK_SIZE];
	unsigned char saved[RAW_BLOCK_SIZE];
	unsigned short seeds[] = { 0x0000, 0x0001, 0x1234, 0x7FFF, 0x2A7B };
	unsigned short u_seeds[] = { 0x0033, 0x2A7B };
	unsigned int u_sectors[] = { 0, 16 };
	size_t i, k;
	unscrambler *u;

	/* ---- edc.txt ---- */
	snprintf(path, sizeof(path), "%s/edc.txt", dir);
	f = fopen(path, "wb");
	if (!f) { perror(path); return 1; }
	{
		unsigned char c1[64];
		unsigned char c2[2060];
		unsigned char c3[2048];
		for (i = 0; i < sizeof(c1); i++) c1[i] = (unsigned char) i;
		memset(c2, 0, sizeof(c2));
		for (i = 0; i < sizeof(c3); i++) c3[i] = (unsigned char) ((i * 7) & 0xff);
		emit_edc(f, c1, sizeof(c1));
		emit_edc(f, c2, sizeof(c2));
		emit_edc(f, c3, sizeof(c3));
		emit_edc(f, c3, 100);
	}
	fclose(f);

	/* ---- lfsr.txt: seed ごとに 64 バイトの系列 ---- */
	snprintf(path, sizeof(path), "%s/lfsr.txt", dir);
	f = fopen(path, "wb");
	if (!f) { perror(path); return 1; }
	for (k = 0; k < sizeof(seeds) / sizeof(seeds[0]); k++) {
		unsigned char seq[64];
		LFSR_init(seeds[k]);
		for (i = 0; i < sizeof(seq); i++)
			seq[i] = LFSR_byte();
		fprintf(f, "%04x ", seeds[k]);
		to_hex(f, seq, sizeof(seq));
		fprintf(f, "\n");
	}
	fclose(f);

	/* ---- unscramble.txt: seed sector disctype raw_hex iso_hex ---- */
	snprintf(path, sizeof(path), "%s/unscramble.txt", dir);
	f = fopen(path, "wb");
	if (!f) { perror(path); return 1; }
	u = unscrambler_new();
	unscrambler_set_disctype(0);
	unscrambler_set_bruteforce(u, true);

	for (k = 0; k < sizeof(u_seeds) / sizeof(u_seeds[0]); k++) {
		/* 決定的な ISO データ (LCG) */
		unsigned int x = 0x12345678u;
		for (i = 0; i < sizeof(buf); i++) {
			x = x * 1664525u + 1013904223u;
			buf[i] = (unsigned char) (x >> 24);
		}
		make_raw(raw, buf, u_seeds[k], u_sectors[k]);
		memcpy(saved, raw, RAW_BLOCK_SIZE);

		if (!unscrambler_unscramble_16sectors(u, u_sectors[k], raw, out)) {
			fprintf(stderr, "unscramble failed for seed %04x\n", u_seeds[k]);
			return 2;
		}
		if (memcmp(out, buf, BLOCK_SIZE) != 0) {
			fprintf(stderr, "unscramble mismatch for seed %04x\n", u_seeds[k]);
			return 3;
		}

		fprintf(f, "%04x %u %d ", u_seeds[k], u_sectors[k], 0);
		to_hex(f, saved, RAW_BLOCK_SIZE);
		fprintf(f, " ");
		to_hex(f, out, BLOCK_SIZE);
		fprintf(f, "\n");
	}
	unscrambler_destroy(u);
	fclose(f);

	printf("wrote vectors to %s\n", dir);
	return 0;
}
