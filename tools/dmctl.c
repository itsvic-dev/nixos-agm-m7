#include <fcntl.h>
#include <linux/dm-ioctl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

static char buf[16384];

static struct dm_ioctl *prep(const char *name, unsigned flags)
{
	struct dm_ioctl *io = (struct dm_ioctl *)buf;

	memset(buf, 0, sizeof(buf));
	io->version[0] = DM_VERSION_MAJOR;
	io->data_size = sizeof(buf);
	io->data_start = sizeof(*io);
	io->flags = flags;
	strncpy(io->name, name, sizeof(io->name) - 1);
	return io;
}

static int create(int fd, const char *name, int ro, const char *table)
{
	unsigned long long start, len;
	char type[DM_MAX_TYPE_NAME];
	struct dm_target_spec *spec;
	struct dm_ioctl *io;
	int off;

	if (sscanf(table, "%llu %llu %15s %n", &start, &len, type, &off) != 3) {
		fprintf(stderr, "bad table\n");
		return 1;
	}
	io = prep(name, 0);
	if (ioctl(fd, DM_DEV_CREATE, io) < 0) {
		perror("DM_DEV_CREATE");
		return 1;
	}
	io = prep(name, ro ? DM_READONLY_FLAG : 0);
	io->target_count = 1;
	spec = (struct dm_target_spec *)(buf + io->data_start);
	spec->sector_start = start;
	spec->length = len;
	strcpy(spec->target_type, type);
	strcpy((char *)(spec + 1), table + off);
	spec->next = sizeof(*spec) + ((strlen(table + off) + 1 + 7) & ~7);
	if (ioctl(fd, DM_TABLE_LOAD, io) < 0) {
		perror("DM_TABLE_LOAD");
		return 1;
	}
	io = prep(name, 0);
	if (ioctl(fd, DM_DEV_SUSPEND, io) < 0) {
		perror("DM_DEV_SUSPEND (resume)");
		return 1;
	}
	printf("/dev/dm-%u\n", (unsigned)(io->dev & 0xff));
	return 0;
}

int main(int argc, char **argv)
{
	int fd = open("/dev/mapper/control", O_RDWR);

	if (fd < 0)
		fd = open("/dev/device-mapper", O_RDWR);
	if (fd < 0) {
		perror("device-mapper control");
		return 1;
	}
	if (argc == 5 && !strcmp(argv[1], "create"))
		return create(fd, argv[2], !strcmp(argv[3], "ro"), argv[4]);
	if (argc == 3 && !strcmp(argv[1], "remove")) {
		if (ioctl(fd, DM_DEV_REMOVE, prep(argv[2], 0)) < 0) {
			perror("DM_DEV_REMOVE");
			return 1;
		}
		return 0;
	}
	fprintf(stderr, "usage: %s create <name> ro|rw '<table>' | remove <name>\n", argv[0]);
	return 2;
}
