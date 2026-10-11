#include <fcntl.h>
#include <linux/dm-ioctl.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(int argc, char **argv)
{
	static char buf[16384];
	struct dm_ioctl *io = (struct dm_ioctl *)buf;
	struct dm_target_spec *spec;
	int fd;

	if (argc != 2) {
		fprintf(stderr, "usage: %s <dm name>\n", argv[0]);
		return 2;
	}
	fd = open("/dev/device-mapper", O_RDWR);
	if (fd < 0) {
		perror("/dev/device-mapper");
		return 1;
	}
	io->version[0] = DM_VERSION_MAJOR;
	io->data_size = sizeof(buf);
	io->data_start = sizeof(*io);
	io->flags = DM_STATUS_TABLE_FLAG;
	strncpy(io->name, argv[1], sizeof(io->name) - 1);
	if (ioctl(fd, DM_TABLE_STATUS, io) < 0) {
		perror("DM_TABLE_STATUS");
		return 1;
	}
	spec = (struct dm_target_spec *)(buf + io->data_start);
	for (unsigned i = 0; i < io->target_count; i++) {
		printf("%llu %llu %s %s\n", (unsigned long long)spec->sector_start,
		       (unsigned long long)spec->length, spec->target_type, (char *)(spec + 1));
		spec = (struct dm_target_spec *)(buf + io->data_start + spec->next);
	}
	return 0;
}
