// usage: pvr-vmtest [/dev/dri/renderD128]
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include <drm/pvr_drm.h>

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
	struct drm_pvr_ioctl_create_bo_args bo = { .size = 1 << 20 };
	struct drm_pvr_ioctl_vm_map_args map = {};
	struct drm_pvr_ioctl_vm_unmap_args unmap = {};
	struct drm_pvr_ioctl_destroy_vm_context_args destroy = {};
	int fd = open(argc > 1 ? argv[1] : "/dev/dri/renderD128", O_RDWR);

	if (fd < 0) {
		perror("open");
		return 1;
	}

	if (do_ioctl(fd, DRM_IOCTL_PVR_DEV_QUERY, &query, "DEV_QUERY heaps") == 0)
		for (unsigned int i = 0; i < heap_info.heaps.count; i++)
			printf("  heap %u: base 0x%010llx size 0x%llx page 2^%u\n", i,
			       (unsigned long long)heaps[i].base, (unsigned long long)heaps[i].size,
			       heaps[i].page_size_log2);

	if (do_ioctl(fd, DRM_IOCTL_PVR_CREATE_VM_CONTEXT, &vm, "CREATE_VM_CONTEXT") ||
	    do_ioctl(fd, DRM_IOCTL_PVR_CREATE_BO, &bo, "CREATE_BO"))
		return 1;

	map.vm_context_handle = vm.handle;
	map.device_addr = heaps[DRM_PVR_HEAP_GENERAL].base;
	map.handle = bo.handle;
	map.size = bo.size;
	if (do_ioctl(fd, DRM_IOCTL_PVR_VM_MAP, &map, "VM_MAP"))
		return 1;

	unmap.vm_context_handle = vm.handle;
	unmap.device_addr = map.device_addr;
	unmap.size = map.size;
	do_ioctl(fd, DRM_IOCTL_PVR_VM_UNMAP, &unmap, "VM_UNMAP");

	destroy.handle = vm.handle;
	do_ioctl(fd, DRM_IOCTL_PVR_DESTROY_VM_CONTEXT, &destroy, "DESTROY_VM_CONTEXT");
	close(fd);

	return 0;
}
