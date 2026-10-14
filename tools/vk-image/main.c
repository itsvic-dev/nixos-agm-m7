// usage: [WIDTH=n] [HEIGHT=n] vk-image load|store
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <vulkan/vulkan.h>

#include "load.spv.h"
#include "store.spv.h"

#define CHECK(expr)                                                         \
	do {                                                                \
		VkResult r_ = (expr);                                       \
		if (r_ != VK_SUCCESS) {                                     \
			fprintf(stderr, "%s failed: %d\n", #expr, r_);      \
			return 1;                                           \
		}                                                           \
	} while (0)

#define MAX(a, b) ((a) > (b) ? (a) : (b))

#define IMAGE_MEM_SIZE (4u << 20)
#define LOAD_TAG 0xaa000000u
#define STORE_TAG 0xbb000000u

static uint32_t twiddle(uint32_t x, uint32_t y, uint32_t w, uint32_t h)
{
	uint32_t bits_x = 0, bits_y = 0, m = 0, out = 0;

	while ((1u << bits_x) < w)
		bits_x++;
	while ((1u << bits_y) < h)
		bits_y++;

	for (uint32_t b = 0; b < MAX(bits_x, bits_y); b++) {
		if (b < bits_y)
			m |= ((y >> b) & 1) << out++;
		if (b < bits_x)
			m |= ((x >> b) & 1) << out++;
	}

	return m;
}

static uint32_t find_memory_type(VkPhysicalDevice phys, uint32_t bits)
{
	VkPhysicalDeviceMemoryProperties props;
	VkMemoryPropertyFlags want = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
				     VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
	uint32_t type = 0;

	vkGetPhysicalDeviceMemoryProperties(phys, &props);
	while (type < props.memoryTypeCount &&
	       (!(bits & (1u << type)) || (props.memoryTypes[type].propertyFlags & want) != want))
		type++;

	return type;
}

static void print_coord(uint32_t v, uint32_t tag)
{
	if ((v & 0xff000000u) == tag)
		printf(" %2u,%-2u", v & 0xff, (v >> 8) & 0xff);
	else if (v == 0)
		printf("    . ");
	else
		printf(" %08x", v);
}

int main(int argc, char **argv)
{
	const uint32_t W = getenv("WIDTH") ? strtoul(getenv("WIDTH"), NULL, 0) : 16;
	const uint32_t H = getenv("HEIGHT") ? strtoul(getenv("HEIGHT"), NULL, 0) : W;
	int store = argc > 1 && !strcmp(argv[1], "store");

	if (argc < 2 || (!store && strcmp(argv[1], "load"))) {
		fprintf(stderr, "usage: [WIDTH=n] [HEIGHT=n] vk-image load|store\n");
		return 2;
	}

	VkInstance instance;
	CHECK(vkCreateInstance(&(VkInstanceCreateInfo){
		.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
		.pApplicationInfo = &(VkApplicationInfo){
			.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
			.apiVersion = VK_API_VERSION_1_2,
		},
	}, NULL, &instance));

	uint32_t n = 1;
	VkPhysicalDevice phys;
	CHECK(vkEnumeratePhysicalDevices(instance, &n, &phys));

	uint32_t qf_count = 8;
	VkQueueFamilyProperties qf[8];
	vkGetPhysicalDeviceQueueFamilyProperties(phys, &qf_count, qf);
	uint32_t family = 0;
	while (family < qf_count && !(qf[family].queueFlags & VK_QUEUE_COMPUTE_BIT))
		family++;

	VkDevice dev;
	CHECK(vkCreateDevice(phys, &(VkDeviceCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
		.queueCreateInfoCount = 1,
		.pQueueCreateInfos = &(VkDeviceQueueCreateInfo){
			.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
			.queueFamilyIndex = family,
			.queueCount = 1,
			.pQueuePriorities = &(float){ 1.0f },
		},
	}, NULL, &dev));
	VkQueue queue;
	vkGetDeviceQueue(dev, family, 0, &queue);

	VkImage image;
	CHECK(vkCreateImage(dev, &(VkImageCreateInfo){
		.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
		.imageType = VK_IMAGE_TYPE_2D,
		.format = VK_FORMAT_R32_UINT,
		.extent = { W, H, 1 },
		.mipLevels = 1,
		.arrayLayers = 1,
		.samples = VK_SAMPLE_COUNT_1_BIT,
		.tiling = VK_IMAGE_TILING_OPTIMAL,
		.usage = VK_IMAGE_USAGE_STORAGE_BIT,
	}, NULL, &image));

	VkMemoryRequirements image_req;
	vkGetImageMemoryRequirements(dev, image, &image_req);
	printf("image %ux%u needs %llu bytes\n", W, H, (unsigned long long)image_req.size);

	VkDeviceSize image_mem_size = MAX(image_req.size, IMAGE_MEM_SIZE);
	VkDeviceMemory image_mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = image_mem_size,
		.memoryTypeIndex = find_memory_type(phys, image_req.memoryTypeBits),
	}, NULL, &image_mem));
	CHECK(vkBindImageMemory(dev, image, image_mem, 0));

	uint32_t *texels;
	CHECK(vkMapMemory(dev, image_mem, 0, image_mem_size, 0, (void **)&texels));
	memset(texels, 0, image_mem_size);
	if (!store)
		for (uint32_t y = 0; y < H; y++)
			for (uint32_t x = 0; x < W; x++)
				texels[twiddle(x, y, W, H)] = LOAD_TAG | y << 8 | x;

	VkImageView view;
	CHECK(vkCreateImageView(dev, &(VkImageViewCreateInfo){
		.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
		.image = image,
		.viewType = VK_IMAGE_VIEW_TYPE_2D,
		.format = VK_FORMAT_R32_UINT,
		.subresourceRange = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1 },
	}, NULL, &view));

	VkDeviceSize out_size = W * H * sizeof(uint32_t);
	VkBuffer out_buf;
	CHECK(vkCreateBuffer(dev, &(VkBufferCreateInfo){
		.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		.size = out_size,
		.usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT,
	}, NULL, &out_buf));
	VkMemoryRequirements out_req;
	vkGetBufferMemoryRequirements(dev, out_buf, &out_req);
	VkDeviceMemory out_mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = out_req.size,
		.memoryTypeIndex = find_memory_type(phys, out_req.memoryTypeBits),
	}, NULL, &out_mem));
	CHECK(vkBindBufferMemory(dev, out_buf, out_mem, 0));
	uint32_t *out;
	CHECK(vkMapMemory(dev, out_mem, 0, out_size, 0, (void **)&out));
	memset(out, 0, out_size);

	VkShaderModule shader;
	CHECK(vkCreateShaderModule(dev, &(VkShaderModuleCreateInfo){
		.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
		.codeSize = store ? sizeof(store_spv) : sizeof(load_spv),
		.pCode = store ? store_spv : load_spv,
	}, NULL, &shader));

	VkDescriptorSetLayout set_layout;
	CHECK(vkCreateDescriptorSetLayout(dev, &(VkDescriptorSetLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
		.bindingCount = 2,
		.pBindings = (VkDescriptorSetLayoutBinding[]){
			{ 0, VK_DESCRIPTOR_TYPE_STORAGE_IMAGE, 1, VK_SHADER_STAGE_COMPUTE_BIT },
			{ 1, VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1, VK_SHADER_STAGE_COMPUTE_BIT },
		},
	}, NULL, &set_layout));

	VkPipelineLayout layout;
	CHECK(vkCreatePipelineLayout(dev, &(VkPipelineLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
		.setLayoutCount = 1,
		.pSetLayouts = &set_layout,
	}, NULL, &layout));

	VkPipeline pipeline;
	CHECK(vkCreateComputePipelines(dev, VK_NULL_HANDLE, 1, &(VkComputePipelineCreateInfo){
		.sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
		.stage = {
			.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
			.stage = VK_SHADER_STAGE_COMPUTE_BIT,
			.module = shader,
			.pName = "main",
		},
		.layout = layout,
	}, NULL, &pipeline));

	VkDescriptorPool pool;
	CHECK(vkCreateDescriptorPool(dev, &(VkDescriptorPoolCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
		.maxSets = 1,
		.poolSizeCount = 2,
		.pPoolSizes = (VkDescriptorPoolSize[]){
			{ VK_DESCRIPTOR_TYPE_STORAGE_IMAGE, 1 },
			{ VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1 },
		},
	}, NULL, &pool));

	VkDescriptorSet set;
	CHECK(vkAllocateDescriptorSets(dev, &(VkDescriptorSetAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
		.descriptorPool = pool,
		.descriptorSetCount = 1,
		.pSetLayouts = &set_layout,
	}, &set));
	vkUpdateDescriptorSets(dev, 2, (VkWriteDescriptorSet[]){
		{
			.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
			.dstSet = set,
			.dstBinding = 0,
			.descriptorCount = 1,
			.descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
			.pImageInfo = &(VkDescriptorImageInfo){
				.imageView = view,
				.imageLayout = VK_IMAGE_LAYOUT_GENERAL,
			},
		},
		{
			.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
			.dstSet = set,
			.dstBinding = 1,
			.descriptorCount = 1,
			.descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
			.pBufferInfo = &(VkDescriptorBufferInfo){ out_buf, 0, out_size },
		},
	}, 0, NULL);

	VkCommandPool cmd_pool;
	CHECK(vkCreateCommandPool(dev, &(VkCommandPoolCreateInfo){
		.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
		.queueFamilyIndex = family,
	}, NULL, &cmd_pool));
	VkCommandBuffer cmd;
	CHECK(vkAllocateCommandBuffers(dev, &(VkCommandBufferAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
		.commandPool = cmd_pool,
		.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY,
		.commandBufferCount = 1,
	}, &cmd));

	CHECK(vkBeginCommandBuffer(cmd, &(VkCommandBufferBeginInfo){
		.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
	}));
	vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_HOST_BIT, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
			     0, 0, NULL, 0, NULL, 1, &(VkImageMemoryBarrier){
				     .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
				     .srcAccessMask = VK_ACCESS_HOST_WRITE_BIT,
				     .dstAccessMask = VK_ACCESS_SHADER_READ_BIT |
						      VK_ACCESS_SHADER_WRITE_BIT,
				     .oldLayout = VK_IMAGE_LAYOUT_UNDEFINED,
				     .newLayout = VK_IMAGE_LAYOUT_GENERAL,
				     .srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
				     .dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
				     .image = image,
				     .subresourceRange = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1 },
			     });
	vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline);
	vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, layout, 0, 1, &set, 0, NULL);
	vkCmdDispatch(cmd, (W + 7) / 8, (H + 7) / 8, 1);
	vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_HOST_BIT,
			     0, 1, &(VkMemoryBarrier){
				     .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
				     .srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT,
				     .dstAccessMask = VK_ACCESS_HOST_READ_BIT,
			     }, 0, NULL, 0, NULL);
	CHECK(vkEndCommandBuffer(cmd));

	VkFence fence;
	CHECK(vkCreateFence(dev, &(VkFenceCreateInfo){
		.sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO,
	}, NULL, &fence));
	CHECK(vkQueueSubmit(queue, 1, &(VkSubmitInfo){
		.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO,
		.commandBufferCount = 1,
		.pCommandBuffers = &cmd,
	}, fence));
	CHECK(vkWaitForFences(dev, 1, &fence, VK_TRUE, 5000000000ull));

	uint32_t bad = 0;
	if (store) {
		uint32_t found = 0;

		printf("texel the store of each coordinate reached (x,y; . = not found):\n");
		for (uint32_t y = 0; y < H; y++) {
			for (uint32_t x = 0; x < W; x++) {
				uint32_t want = STORE_TAG | y << 8 | x;
				uint32_t i = 0;

				while (i < image_mem_size / 4 && texels[i] != want)
					i++;
				if (i < image_mem_size / 4) {
					found++;
					printf(" %6u", i);
				} else {
					printf("      .");
				}
				if (i != twiddle(x, y, W, H))
					bad++;
			}
			printf("\n");
		}
		printf("%u of %u stores found in %u KiB\n", found, W * H,
		       (uint32_t)(image_mem_size >> 10));
	} else {
		printf("texel each coordinate loaded (x,y; . = 0):\n");
		for (uint32_t y = 0; y < H; y++) {
			for (uint32_t x = 0; x < W; x++) {
				print_coord(out[y * W + x], LOAD_TAG);
				if (out[y * W + x] != (LOAD_TAG | y << 8 | x))
					bad++;
			}
			printf("\n");
		}
	}
	printf("%u of %u correct\n", W * H - bad, W * H);

	vkDestroyFence(dev, fence, NULL);
	vkDestroyCommandPool(dev, cmd_pool, NULL);
	vkDestroyDescriptorPool(dev, pool, NULL);
	vkDestroyPipeline(dev, pipeline, NULL);
	vkDestroyPipelineLayout(dev, layout, NULL);
	vkDestroyDescriptorSetLayout(dev, set_layout, NULL);
	vkDestroyShaderModule(dev, shader, NULL);
	vkUnmapMemory(dev, out_mem);
	vkFreeMemory(dev, out_mem, NULL);
	vkDestroyBuffer(dev, out_buf, NULL);
	vkDestroyImageView(dev, view, NULL);
	vkUnmapMemory(dev, image_mem);
	vkFreeMemory(dev, image_mem, NULL);
	vkDestroyImage(dev, image, NULL);
	vkDestroyDevice(dev, NULL);
	vkDestroyInstance(instance, NULL);

	return bad ? 1 : 0;
}
