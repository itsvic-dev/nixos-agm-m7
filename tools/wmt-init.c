// SPDX-License-Identifier: GPL-2.0-only
/* Bring up the MT6739 CONSYS firmware over /sys/kernel/debug/mt6739-consys/btif. */

#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define BTIF "/sys/kernel/debug/mt6739-consys/btif"
#define TYPE_WMT 4
#define PATCH_HDR 28
#define FRAG 1000

static int fd;
static int full;
static unsigned int txseq, txack = 7, rxseq;
static uint8_t rxbuf[8192];
static size_t rxlen;

static void die(const char *fmt, ...)
{
	va_list ap;

	va_start(ap, fmt);
	vfprintf(stderr, fmt, ap);
	va_end(ap);
	fputc('\n', stderr);
	exit(1);
}

static void hexdump(const char *tag, const uint8_t *p, size_t n)
{
	fprintf(stderr, "%s", tag);
	for (size_t i = 0; i < n && i < 32; i++)
		fprintf(stderr, " %02x", p[i]);
	fprintf(stderr, n > 32 ? " ...\n" : "\n");
}

static uint16_t crc16_arc(const uint8_t *p, size_t n)
{
	uint16_t crc = 0;

	while (n--) {
		crc ^= *p++;
		for (int i = 0; i < 8; i++)
			crc = crc & 1 ? (crc >> 1) ^ 0xa001 : crc >> 1;
	}
	return crc;
}

static void put(const uint8_t *p, size_t n)
{
	while (n) {
		ssize_t r = write(fd, p, n);

		if (r < 0)
			die("btif write: %s", strerror(errno));
		p += r;
		n -= r;
	}
}

static void send_frame(int type, const uint8_t *payload, size_t len)
{
	uint8_t hdr[4];
	uint16_t crc = 0;

	if (full) {
		hdr[0] = 0x80 | txseq << 3 | txack;
		txseq = (txseq + 1) & 7;
		crc = crc16_arc(payload, len);
	} else {
		hdr[0] = 0x80;
	}
	hdr[1] = type << 4 | len >> 8;
	hdr[2] = len & 0xff;
	hdr[3] = full ? (hdr[0] + hdr[1] + hdr[2]) & 0xff : 0;
	put(hdr, 4);
	put(payload, len);
	put((uint8_t []){ crc & 0xff, crc >> 8 }, 2);
}

static void send_ack(void)
{
	uint8_t hdr[4] = { 0x80 | txack, 0, 0, 0x80 | txack };

	put(hdr, 4);
}

static void fill(void)
{
	ssize_t r = read(fd, rxbuf + rxlen, sizeof(rxbuf) - rxlen);

	if (r < 0)
		die("btif read: %s", strerror(errno));
	if (r == 0)
		usleep(1000);
	rxlen += r;
}

/* Returns the payload length of the next WMT packet, copied to out. */
static size_t recv_wmt(uint8_t *out, int timeout_ms)
{
	struct timespec start, now;

	clock_gettime(CLOCK_MONOTONIC, &start);
	for (;;) {
		while (rxlen && rxbuf[0] != 0x80 && !(full && (rxbuf[0] & 0xc0) == 0x80)) {
			memmove(rxbuf, rxbuf + 1, --rxlen);
		}
		if (rxlen >= 4) {
			size_t len = (rxbuf[1] & 0xf) << 8 | rxbuf[2];
			int type = rxbuf[1] >> 4;

			if (full && len == 0) {
				memmove(rxbuf, rxbuf + 4, rxlen -= 4);
				continue;
			}
			if (rxlen >= 4 + len + 2) {
				unsigned int seq = (rxbuf[0] >> 3) & 7;

				memcpy(out, rxbuf + 4, len);
				memmove(rxbuf, rxbuf + 6 + len, rxlen -= 6 + len);
				if (full) {
					if (seq == rxseq) {
						txack = seq;
						rxseq = (rxseq + 1) & 7;
					}
					send_ack();
				}
				if (type == TYPE_WMT)
					return len;
				hexdump(type == 5 || type == 6 ? "firmware:" : "other:", out, len);
				continue;
			}
		}
		clock_gettime(CLOCK_MONOTONIC, &now);
		if ((now.tv_sec - start.tv_sec) * 1000 + (now.tv_nsec - start.tv_nsec) / 1000000 > timeout_ms)
			return 0;
		fill();
	}
}

static void cmd(const char *what, const uint8_t *c, size_t clen, const uint8_t *evt, size_t elen)
{
	uint8_t buf[2048];
	size_t n;

	send_frame(TYPE_WMT, c, clen);
	n = recv_wmt(buf, 2000);
	if (!n)
		die("%s: no reply", what);
	if (evt && (n != elen || memcmp(buf, evt, elen))) {
		hexdump("got:", buf, n);
		die("%s: unexpected reply", what);
	}
	fprintf(stderr, "ok: %s\n", what);
}

#define CMD(what, c, e) cmd(what, c, sizeof(c), e, sizeof(e))

static uint32_t reg_read(uint32_t addr)
{
	uint8_t c[20] = { 0x01, 0x08, 0x10, 0x00, 0x02, 0x01, 0x00, 0x01,
			  addr, addr >> 8, addr >> 16, addr >> 24, 0, 0, 0, 0,
			  0xff, 0xff, 0xff, 0xff };
	uint8_t buf[64];
	size_t n;

	send_frame(TYPE_WMT, c, sizeof(c));
	n = recv_wmt(buf, 2000);
	if (n != 16 || buf[0] != 2 || buf[1] != 8)
		die("read 0x%08x failed", addr);
	return buf[12] | buf[13] << 8 | buf[14] << 16 | (uint32_t)buf[15] << 24;
}

static void reg_write(uint32_t addr, uint32_t val, uint32_t mask)
{
	uint8_t c[20] = { 0x01, 0x08, 0x10, 0x00, 0x01, 0x01, 0x00, 0x01,
			  addr, addr >> 8, addr >> 16, addr >> 24,
			  val, val >> 8, val >> 16, val >> 24,
			  mask, mask >> 8, mask >> 16, mask >> 24 };
	static const uint8_t evt[] = { 0x02, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x01 };
	char what[64];

	snprintf(what, sizeof(what), "write 0x%08x = 0x%08x", addr, val);
	cmd(what, c, sizeof(c), evt, sizeof(evt));
}

static void patch(const char *dir, const char *name)
{
	static const uint8_t evt[] = { 0x02, 0x01, 0x01, 0x00, 0x00 };
	static const uint8_t reset[] = { 0x01, 0x07, 0x01, 0x00, 0x04 };
	static const uint8_t reset_evt[] = { 0x02, 0x07, 0x01, 0x00, 0x00 };
	static uint8_t file[1 << 20];
	uint8_t frag[5 + FRAG];
	char path[512];
	size_t size, off;
	FILE *f;

	snprintf(path, sizeof(path), "%s/%s", dir, name);
	f = fopen(path, "rb");
	if (!f)
		die("%s: %s", path, strerror(errno));
	size = fread(file, 1, sizeof(file), f);
	fclose(f);
	if (size <= PATCH_HDR)
		die("%s: too short", path);

	reg_write(0x02090240, 0, 0xffffffff);
	reg_write(0x020904c8, (uint32_t)file[0x1a] << 16 | (uint32_t)file[0x1b] << 24, 0xffffffff);

	for (off = PATCH_HDR; off < size; off += FRAG) {
		size_t n = size - off < FRAG ? size - off : FRAG;

		frag[0] = 0x01;
		frag[1] = 0x01;
		frag[2] = (n + 1) & 0xff;
		frag[3] = (n + 1) >> 8;
		frag[4] = off == PATCH_HDR ? 1 : off + n >= size ? 3 : 2;
		memcpy(frag + 5, file + off, n);
		send_frame(TYPE_WMT, frag, 5 + n);
		{
			uint8_t buf[64];
			size_t r = recv_wmt(buf, 2000);

			if (r != sizeof(evt) || memcmp(buf, evt, sizeof(evt))) {
				hexdump("got:", buf, r);
				die("%s: fragment at %zu failed", name, off);
			}
		}
	}
	fprintf(stderr, "ok: %s, %zu bytes\n", name, size - PATCH_HDR);
	cmd("reset", reset, sizeof(reset), reset_evt, sizeof(reset_evt));
}

int main(int argc, char **argv)
{
	const char *dir = argc > 1 ? argv[1] : "/root/fw";
	static const uint8_t query_stp[] = { 0x01, 0x04, 0x01, 0x00, 0x04 };
	static const uint8_t query_mand[] = { 0x02, 0x04, 0x06, 0x00, 0x00, 0x04, 0x11, 0x00, 0x00, 0x00 };
	static const uint8_t set_stp[] = { 0x01, 0x04, 0x05, 0x00, 0x03, 0xdf, 0x0e, 0x68, 0x01 };
	static const uint8_t set_stp_evt[] = { 0x02, 0x04, 0x02, 0x00, 0x00, 0x03 };
	static const uint8_t query_full[] = { 0x02, 0x04, 0x06, 0x00, 0x00, 0x04, 0xdf, 0x0e, 0x68, 0x01 };
	static const uint8_t coex_evt[] = { 0x02, 0x10, 0x01, 0x00, 0x00 };
	static const uint8_t coex_filter[] = {
		0x01, 0x10, 0x45, 0x00, 0x11, 0x00, 0x00, 0x01, 0x00, 0x11, 0x11, 0x16, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x63, 0x63, 0x63, 0x00, 0x39, 0x43, 0x63, 0x63, 0x02, 0x02, 0x03, 0x00, 0x01, 0x01, 0x01,
		0x01, 0x0e, 0x0e, 0x0e, 0x00, 0x0a, 0x0c, 0x0e, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	};
	static const uint8_t coex_lte_freq[] = {
		0x01, 0x10, 0x21, 0x00, 0x12, 0xfc, 0x08, 0x15, 0x09, 0x2e, 0x09, 0x47, 0x09, 0xc4, 0x09, 0xd4,
		0x09, 0xe3, 0x09, 0x5a, 0x0a, 0x14, 0x09, 0x2d, 0x09, 0x46, 0x09, 0x60, 0x09, 0xd3, 0x09, 0xe2,
		0x09, 0x59, 0x0a, 0x8b, 0x0a,
	};
	static const uint8_t coex_lte_15[] = { 0x01, 0x10, 0x02, 0x00, 0x15, 0x01 };
	static const uint8_t cal[] = { 0x01, 0x14, 0x01, 0x00, 0x01 };
	static const uint8_t coex_ant[] = { 0x01, 0x10, 0x02, 0x00, 0x06, 0x01 };
	static const uint8_t coex_wifi_path[] = { 0x01, 0x10, 0x03, 0x00, 0x1a, 0x00, 0x00 };
	static const uint8_t coex_1b[] = { 0x01, 0x10, 0x12, 0x00, 0x1b, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 };
	static const uint8_t coex_1d[] = { 0x01, 0x10, 0x02, 0x00, 0x1d, 0x00 };
	static const uint8_t co_clock[] = { 0x01, 0x0a, 0x02, 0x00, 0x08, 0x03 };
	static const uint8_t co_clock_evt[] = { 0x02, 0x0a, 0x01, 0x00, 0x00 };
	static const uint8_t fm_strap[] = { 0x01, 0x05, 0x02, 0x00, 0x02, 0x02 };
	static const uint8_t fm_strap_evt[] = { 0x02, 0x05, 0x02, 0x00, 0x00, 0x02 };
	uint8_t buf[64];
	size_t n;

	fd = open(BTIF, O_RDWR);
	if (fd < 0)
		die("%s: %s", BTIF, strerror(errno));
	while (read(fd, rxbuf, sizeof(rxbuf)) > 0)
		;

	fprintf(stderr, "hw 0x%x, ver 0x%x, rom 0x%x\n", reg_read(0x80000008),
		reg_read(0x80000000), reg_read(0x80000004));
	CMD("query stp (mand)", query_stp, query_mand);
	CMD("set stp", set_stp, set_stp_evt);
	full = 1;
	usleep(10000);
	CMD("query stp (full)", query_stp, query_full);

	reg_write(0x80100060, 0, 0x00000f00);
	reg_write(0x80100060, 0, 0x000000f0);
	reg_write(0x80100060, 0, 0x00000008);
	reg_write(0x80000334, 0x00010000, 0xffffffff);
	reg_write(0x8000010c, 0x00844d59, 0xffffffff);

	patch(dir, "ROMv2_lm_patch_1_1_hdr.bin");
	patch(dir, "ROMv2_lm_patch_1_0_hdr.bin");

	reg_write(0x8000010c, 0x00844d00, 0xffffffff);
	reg_write(0x80000334, 0, 0xffffffff);

	usleep(5000);
	CMD("coex filter", coex_filter, coex_evt);
	CMD("coex lte freq", coex_lte_freq, coex_evt);
	CMD("coex lte 0x15", coex_lte_15, coex_evt);

	send_frame(TYPE_WMT, cal, sizeof(cal));
	n = recv_wmt(buf, 3000);
	if (n < 4 || buf[0] != 2 || buf[1] != 0x14)
		die("calibration failed");
	hexdump("ok: calibration:", buf, n);

	CMD("coex ant mode", coex_ant, coex_evt);
	CMD("coex wifi path", coex_wifi_path, coex_evt);
	CMD("coex 0x1b", coex_1b, coex_evt);
	CMD("coex 0x1d", coex_1d, coex_evt);
	CMD("co-clock", co_clock, co_clock_evt);
	CMD("fm strap", fm_strap, fm_strap_evt);

	fprintf(stderr, "done\n");
	return 0;
}
