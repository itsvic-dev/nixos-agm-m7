// usage: pvr-computetest [/dev/dri/renderD128]
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#include <drm/drm.h>
#include <drm/pvr_drm.h>

#define CDM_STREAM_TERMINATE 0x80000000u

static int do_ioctl(int fd, unsigned long req, void *arg, const char *name)
{
	int ret;

	do {
		ret = ioctl(fd, req, arg);
	} while (ret < 0 && errno == EINTR);
	printf("%-20s %s\n", name, ret < 0 ? strerror(errno) : "ok");
	return ret;
}

int main(int argc, char **argv)
{
	struct drm_pvr_heap heaps[DRM_PVR_HEAP_COUNT] = {};
	struct drm_pvr_dev_query_heap_info heap_info = {
		.heaps = { .stride = sizeof(heaps[0]), .count = DRM_PVR_HEAP_COUNT,
			   .array = (uintptr_t)heaps },
	};
	struct drm_pvr_ioctl_dev_query_args query = {
		.type = DRM_PVR_DEV_QUERY_HEAP_INFO_GET,
		.size = sizeof(heap_info),
		.pointer = (uintptr_t)&heap_info,
	};
	struct drm_pvr_ioctl_create_vm_context_args vm = {};
	struct drm_pvr_ioctl_create_bo_args bo = {
		.size = 4096,
		.flags = DRM_PVR_BO_ALLOW_CPU_USERSPACE_ACCESS,
	};
	struct drm_pvr_ioctl_get_bo_mmap_offset_args mmap_offset = {};
	struct drm_pvr_ioctl_vm_map_args map = {};
	uint64_t static_state[8] = { sizeof(static_state) };
	struct drm_pvr_ioctl_create_context_args ctx = {
		.type = DRM_PVR_CTX_TYPE_COMPUTE,
		.static_context_state = (uintptr_t)static_state,
		.static_context_state_len = sizeof(static_state),
	};
	struct {
		uint32_t len, padding;
		uint64_t tpu_border_colour_table;
		uint64_t cdm_ctrl_stream_base;
		uint64_t cdm_context_state_base_addr;
		uint32_t cdm_resume_pds1;
	} __attribute__((packed)) stream = { .len = sizeof(stream) };
	struct drm_syncobj_create syncobj = {};
	struct drm_pvr_sync_op sync_op = { .flags = DRM_PVR_SYNC_OP_FLAG_SIGNAL };
	struct drm_pvr_job job = {
		.type = DRM_PVR_JOB_TYPE_COMPUTE,
		.cmd_stream = (uintptr_t)&stream,
		.cmd_stream_len = sizeof(stream),
		.sync_ops = { .stride = sizeof(sync_op), .count = 1, .array = (uintptr_t)&sync_op },
	};
	struct drm_pvr_ioctl_submit_jobs_args submit = {
		.jobs = { .stride = sizeof(job), .count = 1, .array = (uintptr_t)&job },
	};
	struct drm_syncobj_wait wait = { .count_handles = 1 };
	struct timespec ts;
	uint32_t *cs;
	int fd = open(argc > 1 ? argv[1] : "/dev/dri/renderD128", O_RDWR);

	if (fd < 0) {
		perror("open");
		return 1;
	}

	if (do_ioctl(fd, DRM_IOCTL_PVR_DEV_QUERY, &query, "DEV_QUERY heaps") ||
	    do_ioctl(fd, DRM_IOCTL_PVR_CREATE_VM_CONTEXT, &vm, "CREATE_VM_CONTEXT") ||
	    do_ioctl(fd, DRM_IOCTL_PVR_CREATE_BO, &bo, "CREATE_BO"))
		return 1;

	mmap_offset.handle = bo.handle;
	if (do_ioctl(fd, DRM_IOCTL_PVR_GET_BO_MMAP_OFFSET, &mmap_offset, "GET_BO_MMAP_OFFSET"))
		return 1;
	cs = mmap(NULL, bo.size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, mmap_offset.offset);
	if (cs == MAP_FAILED) {
		perror("mmap");
		return 1;
	}
	cs[0] = CDM_STREAM_TERMINATE;
	munmap(cs, bo.size);

	map.vm_context_handle = vm.handle;
	map.device_addr = heaps[DRM_PVR_HEAP_GENERAL].base;
	map.handle = bo.handle;
	map.size = bo.size;
	if (do_ioctl(fd, DRM_IOCTL_PVR_VM_MAP, &map, "VM_MAP"))
		return 1;

	ctx.vm_context_handle = vm.handle;
	if (do_ioctl(fd, DRM_IOCTL_PVR_CREATE_CONTEXT, &ctx, "CREATE_CONTEXT") ||
	    do_ioctl(fd, DRM_IOCTL_SYNCOBJ_CREATE, &syncobj, "SYNCOBJ_CREATE"))
		return 1;

	stream.cdm_ctrl_stream_base = map.device_addr;
	job.context_handle = ctx.handle;
	sync_op.handle = syncobj.handle;
	if (do_ioctl(fd, DRM_IOCTL_PVR_SUBMIT_JOBS, &submit, "SUBMIT_JOBS"))
		return 1;

	clock_gettime(CLOCK_MONOTONIC, &ts);
	wait.handles = (uintptr_t)&syncobj.handle;
	wait.timeout_nsec = (ts.tv_sec + 5) * 1000000000ll + ts.tv_nsec;
	if (do_ioctl(fd, DRM_IOCTL_SYNCOBJ_WAIT, &wait, "SYNCOBJ_WAIT"))
		return 1;

	close(fd);

	return 0;
}
