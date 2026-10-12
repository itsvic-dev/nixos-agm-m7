// usage: [WIDTH=n] [HEIGHT=n] [RAW=1] [PAUSE=s] [TILE=x,y] [DEPTH=1|2|3] [UBO=1] [Z=depth] [TEX=1|2] [VARY=1] [DERIV=1] [PROBE=n] vk-triangle [clear|store]
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <vulkan/vulkan.h>

#include "probe_frag.h"
#include "probe_vert.h"
#include "read_comp.h"
#include "store_comp.h"
#include "tri_frag.h"
#include "tri_deriv_frag.h"
#include "tri_tex_frag.h"
#include "tri_vary_frag.h"
#include "tri_vary_vert.h"
#include "tri_ubo_vert.h"
#include "tri_vert.h"

#define RED 0xff0000ffu
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define DIV_ROUND_UP(a, b) (((a) + (b) - 1) / (b))
#define BLUE 0xffff0000u
#define GREEN 0xff00ff00u
#define YELLOW 0xff00ffffu

#define CHECK(expr)                                                         \
	do {                                                                \
		VkResult r_ = (expr);                                       \
		if (r_ != VK_SUCCESS) {                                     \
			fprintf(stderr, "%s failed: %d\n", #expr, r_);      \
			return 1;                                           \
		}                                                           \
	} while (0)

static uint32_t find_memory(VkPhysicalDevice phys, uint32_t bits, VkMemoryPropertyFlags want)
{
	VkPhysicalDeviceMemoryProperties props;
	uint32_t type = 0;

	vkGetPhysicalDeviceMemoryProperties(phys, &props);
	while (type < props.memoryTypeCount &&
	       (!(bits & (1u << type)) || (props.memoryTypes[type].propertyFlags & want) != want))
		type++;

	return type;
}

static int has_instance_ext(const char *name)
{
	VkExtensionProperties props[64];
	uint32_t n = 64;

	vkEnumerateInstanceExtensionProperties(NULL, &n, props);
	for (uint32_t i = 0; i < n; i++)
		if (!strcmp(props[i].extensionName, name))
			return 1;

	return 0;
}

static int has_device_ext(VkPhysicalDevice phys, const char *name)
{
	VkExtensionProperties props[256];
	uint32_t n = 256;

	vkEnumerateDeviceExtensionProperties(phys, NULL, &n, props);
	for (uint32_t i = 0; i < n; i++)
		if (!strcmp(props[i].extensionName, name))
			return 1;

	return 0;
}

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

static VkShaderModule shader(VkDevice dev, const uint32_t *code, size_t size)
{
	VkShaderModule module = VK_NULL_HANDLE;

	vkCreateShaderModule(dev, &(VkShaderModuleCreateInfo){
		.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
		.codeSize = size,
		.pCode = code,
	}, NULL, &module);

	return module;
}

int main(int argc, char **argv)
{
	const uint32_t W = getenv("WIDTH") ? strtoul(getenv("WIDTH"), NULL, 0) : 64;
	const uint32_t H = getenv("HEIGHT") ? strtoul(getenv("HEIGHT"), NULL, 0) : W;
	int draw = argc < 2;
	int store = argc > 1 && !strcmp(argv[1], "store");

	const char *portability_ext = VK_KHR_PORTABILITY_ENUMERATION_EXTENSION_NAME;
	int portability = has_instance_ext(portability_ext);

	VkInstance instance;
	CHECK(vkCreateInstance(&(VkInstanceCreateInfo){
		.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
		.flags = portability ? VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR : 0,
		.pApplicationInfo = &(VkApplicationInfo){
			.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
			.apiVersion = VK_API_VERSION_1_2,
		},
		.enabledExtensionCount = portability,
		.ppEnabledExtensionNames = &portability_ext,
	}, NULL, &instance));

	uint32_t n = 1;
	VkPhysicalDevice phys;
	CHECK(vkEnumeratePhysicalDevices(instance, &n, &phys));

	uint32_t qf_count = 8;
	VkQueueFamilyProperties qf[8];
	vkGetPhysicalDeviceQueueFamilyProperties(phys, &qf_count, qf);
	const VkQueueFlags need = VK_QUEUE_GRAPHICS_BIT | VK_QUEUE_COMPUTE_BIT;
	uint32_t family = 0;
	while (family < qf_count && (qf[family].queueFlags & need) != need)
		family++;

	const char *subset_ext = "VK_KHR_portability_subset";
	int subset = has_device_ext(phys, subset_ext);

	VkDevice dev;
	CHECK(vkCreateDevice(phys, &(VkDeviceCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
		.enabledExtensionCount = subset,
		.ppEnabledExtensionNames = &subset_ext,
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
		.format = VK_FORMAT_R8G8B8A8_UNORM,
		.extent = { W, H, 1 },
		.mipLevels = 1,
		.arrayLayers = 1,
		.samples = VK_SAMPLE_COUNT_1_BIT,
		.tiling = VK_IMAGE_TILING_OPTIMAL,
		.usage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_STORAGE_BIT,
	}, NULL, &image));
	VkMemoryRequirements req;
	vkGetImageMemoryRequirements(dev, image, &req);
	VkDeviceMemory image_mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = getenv("RAW") ? 64 << 20 : req.size,
		.memoryTypeIndex = find_memory(phys, req.memoryTypeBits,
					       VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT),
	}, NULL, &image_mem));
	VkDeviceSize image_size = getenv("RAW") ? 64 << 20 : req.size;
	VkDeviceSize image_offset = getenv("RAW") ? 32 << 20 : 0;
	printf("image needs %llu bytes\n", (unsigned long long)req.size);
	CHECK(vkBindImageMemory(dev, image, image_mem, image_offset));

	VkImageView view;
	CHECK(vkCreateImageView(dev, &(VkImageViewCreateInfo){
		.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
		.image = image,
		.viewType = VK_IMAGE_VIEW_TYPE_2D,
		.format = VK_FORMAT_R8G8B8A8_UNORM,
		.subresourceRange = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1 },
	}, NULL, &view));

	VkDeviceSize size = (W * H + 2) * sizeof(uint32_t);
	VkBuffer buf;
	CHECK(vkCreateBuffer(dev, &(VkBufferCreateInfo){
		.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		.size = size,
		.usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT,
	}, NULL, &buf));
	vkGetBufferMemoryRequirements(dev, buf, &req);
	VkDeviceMemory buf_mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = req.size,
		.memoryTypeIndex = find_memory(phys, req.memoryTypeBits,
					       VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
					       VK_MEMORY_PROPERTY_HOST_COHERENT_BIT),
	}, NULL, &buf_mem));
	CHECK(vkBindBufferMemory(dev, buf, buf_mem, 0));
	uint32_t *pixels;
	CHECK(vkMapMemory(dev, buf_mem, 0, size, 0, (void **)&pixels));
	memset(pixels, 0xaa, size);

	int depth = getenv("DEPTH") ? atoi(getenv("DEPTH")) : 0;
	VkImageView views[2] = { view, VK_NULL_HANDLE };
	VkFormat depth_format = depth == 3 ? VK_FORMAT_D32_SFLOAT : VK_FORMAT_D16_UNORM;

	if (depth) {
		VkImage depth_image;
		CHECK(vkCreateImage(dev, &(VkImageCreateInfo){
			.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
			.imageType = VK_IMAGE_TYPE_2D,
			.format = depth_format,
			.extent = { W, H, 1 },
			.mipLevels = 1,
			.arrayLayers = 1,
			.samples = VK_SAMPLE_COUNT_1_BIT,
			.tiling = VK_IMAGE_TILING_OPTIMAL,
			.usage = VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT,
		}, NULL, &depth_image));
		vkGetImageMemoryRequirements(dev, depth_image, &req);
		VkDeviceMemory depth_mem;
		CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
			.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
			.allocationSize = req.size,
			.memoryTypeIndex = find_memory(phys, req.memoryTypeBits, 0),
		}, NULL, &depth_mem));
		CHECK(vkBindImageMemory(dev, depth_image, depth_mem, 0));
		CHECK(vkCreateImageView(dev, &(VkImageViewCreateInfo){
			.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
			.image = depth_image,
			.viewType = VK_IMAGE_VIEW_TYPE_2D,
			.format = depth_format,
			.subresourceRange = { VK_IMAGE_ASPECT_DEPTH_BIT, 0, 1, 0, 1 },
		}, NULL, &views[1]));
	}

	VkRenderPass pass;
	CHECK(vkCreateRenderPass(dev, &(VkRenderPassCreateInfo){
		.sType = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
		.attachmentCount = depth ? 2 : 1,
		.pAttachments = (VkAttachmentDescription[]){
			{
				.format = VK_FORMAT_R8G8B8A8_UNORM,
				.samples = VK_SAMPLE_COUNT_1_BIT,
				.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
				.storeOp = VK_ATTACHMENT_STORE_OP_STORE,
				.stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE,
				.stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
				.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
				.finalLayout = VK_IMAGE_LAYOUT_GENERAL,
			},
			{
				.format = depth_format,
				.samples = VK_SAMPLE_COUNT_1_BIT,
				.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
				.storeOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
				.stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE,
				.stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
				.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
				.finalLayout = VK_IMAGE_LAYOUT_DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
			},
		},
		.subpassCount = 1,
		.pSubpasses = &(VkSubpassDescription){
			.pipelineBindPoint = VK_PIPELINE_BIND_POINT_GRAPHICS,
			.colorAttachmentCount = 1,
			.pColorAttachments = &(VkAttachmentReference){
				0, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
			},
			.pDepthStencilAttachment = depth ? &(VkAttachmentReference){
				1, VK_IMAGE_LAYOUT_DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
			} : NULL,
		},
	}, NULL, &pass));

	VkFramebuffer fb;
	CHECK(vkCreateFramebuffer(dev, &(VkFramebufferCreateInfo){
		.sType = VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
		.renderPass = pass,
		.attachmentCount = depth ? 2 : 1,
		.pAttachments = views,
		.width = W,
		.height = H,
		.layers = 1,
	}, NULL, &fb));

	uint32_t vp_x = 0, vp_y = 0, vp_w = W, vp_h = H;

	if (getenv("TILE")) {
		sscanf(getenv("TILE"), "%u,%u", &vp_x, &vp_y);
		vp_x *= 16;
		vp_y *= 16;
		vp_w = vp_h = 16;
	}

	int ubo = getenv("UBO") != NULL;
	VkDescriptorSetLayout ubo_layout;
	CHECK(vkCreateDescriptorSetLayout(dev, &(VkDescriptorSetLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
		.bindingCount = 1,
		.pBindings = &(VkDescriptorSetLayoutBinding){
			0, VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER, 1, VK_SHADER_STAGE_VERTEX_BIT,
		},
	}, NULL, &ubo_layout));

	const float z = getenv("Z") ? strtof(getenv("Z"), NULL) : 0.5f;
	const float positions[3][4] = {
		{ 0.0f, -0.75f, z, 1.0f },
		{ 0.75f, 0.75f, z, 1.0f },
		{ -0.75f, 0.75f, z, 1.0f },
	};
	VkBuffer ubo_buf;
	CHECK(vkCreateBuffer(dev, &(VkBufferCreateInfo){
		.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		.size = sizeof(positions),
		.usage = VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT,
	}, NULL, &ubo_buf));
	vkGetBufferMemoryRequirements(dev, ubo_buf, &req);
	VkDeviceMemory ubo_mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = req.size,
		.memoryTypeIndex = find_memory(phys, req.memoryTypeBits,
					       VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
					       VK_MEMORY_PROPERTY_HOST_COHERENT_BIT),
	}, NULL, &ubo_mem));
	CHECK(vkBindBufferMemory(dev, ubo_buf, ubo_mem, 0));
	void *ubo_map;
	CHECK(vkMapMemory(dev, ubo_mem, 0, sizeof(positions), 0, &ubo_map));
	memcpy(ubo_map, positions, sizeof(positions));

	VkDescriptorPool ubo_pool;
	CHECK(vkCreateDescriptorPool(dev, &(VkDescriptorPoolCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
		.maxSets = 1,
		.poolSizeCount = 1,
		.pPoolSizes = &(VkDescriptorPoolSize){ VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER, 1 },
	}, NULL, &ubo_pool));
	VkDescriptorSet ubo_set;
	CHECK(vkAllocateDescriptorSets(dev, &(VkDescriptorSetAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
		.descriptorPool = ubo_pool,
		.descriptorSetCount = 1,
		.pSetLayouts = &ubo_layout,
	}, &ubo_set));
	vkUpdateDescriptorSets(dev, 1, &(VkWriteDescriptorSet){
		.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
		.dstSet = ubo_set,
		.descriptorCount = 1,
		.descriptorType = VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
		.pBufferInfo = &(VkDescriptorBufferInfo){ ubo_buf, 0, sizeof(positions) },
	}, 0, NULL);

	int tex = getenv("TEX") ? atoi(getenv("TEX")) : 0;
	VkImage tex_image;
	CHECK(vkCreateImage(dev, &(VkImageCreateInfo){
		.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
		.imageType = VK_IMAGE_TYPE_2D,
		.format = VK_FORMAT_R8G8B8A8_UNORM,
		.extent = { 64, 64, 1 },
		.mipLevels = 1,
		.arrayLayers = 1,
		.samples = VK_SAMPLE_COUNT_1_BIT,
		.tiling = VK_IMAGE_TILING_OPTIMAL,
		.usage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT |
			 VK_IMAGE_USAGE_TRANSFER_DST_BIT,
	}, NULL, &tex_image));
	vkGetImageMemoryRequirements(dev, tex_image, &req);
	VkDeviceMemory tex_mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = req.size,
		.memoryTypeIndex = find_memory(phys, req.memoryTypeBits, 0),
	}, NULL, &tex_mem));
	CHECK(vkBindImageMemory(dev, tex_image, tex_mem, 0));
	VkImageView tex_view;
	CHECK(vkCreateImageView(dev, &(VkImageViewCreateInfo){
		.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
		.image = tex_image,
		.viewType = VK_IMAGE_VIEW_TYPE_2D,
		.format = VK_FORMAT_R8G8B8A8_UNORM,
		.subresourceRange = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1 },
	}, NULL, &tex_view));

	VkBuffer staging;
	CHECK(vkCreateBuffer(dev, &(VkBufferCreateInfo){
		.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		.size = 64 * 64 * 4,
		.usage = VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
	}, NULL, &staging));
	vkGetBufferMemoryRequirements(dev, staging, &req);
	VkDeviceMemory staging_mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = req.size,
		.memoryTypeIndex = find_memory(phys, req.memoryTypeBits,
					       VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
					       VK_MEMORY_PROPERTY_HOST_COHERENT_BIT),
	}, NULL, &staging_mem));
	CHECK(vkBindBufferMemory(dev, staging, staging_mem, 0));
	uint32_t *staging_map;
	CHECK(vkMapMemory(dev, staging_mem, 0, 64 * 64 * 4, 0, (void **)&staging_map));
	for (int i = 0; i < 64 * 64; i++)
		staging_map[i] = GREEN;

	VkRenderPass tex_pass;
	CHECK(vkCreateRenderPass(dev, &(VkRenderPassCreateInfo){
		.sType = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
		.attachmentCount = 1,
		.pAttachments = &(VkAttachmentDescription){
			.format = VK_FORMAT_R8G8B8A8_UNORM,
			.samples = VK_SAMPLE_COUNT_1_BIT,
			.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
			.storeOp = VK_ATTACHMENT_STORE_OP_STORE,
			.stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE,
			.stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
			.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
			.finalLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
		},
		.subpassCount = 1,
		.pSubpasses = &(VkSubpassDescription){
			.pipelineBindPoint = VK_PIPELINE_BIND_POINT_GRAPHICS,
			.colorAttachmentCount = 1,
			.pColorAttachments = &(VkAttachmentReference){
				0, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
			},
		},
	}, NULL, &tex_pass));
	VkFramebuffer tex_fb;
	CHECK(vkCreateFramebuffer(dev, &(VkFramebufferCreateInfo){
		.sType = VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
		.renderPass = tex_pass,
		.attachmentCount = 1,
		.pAttachments = &tex_view,
		.width = 64,
		.height = 64,
		.layers = 1,
	}, NULL, &tex_fb));

	VkSampler sampler;
	CHECK(vkCreateSampler(dev, &(VkSamplerCreateInfo){
		.sType = VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
		.magFilter = VK_FILTER_NEAREST,
		.minFilter = VK_FILTER_NEAREST,
		.addressModeU = VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
		.addressModeV = VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
		.addressModeW = VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
	}, NULL, &sampler));
	VkDescriptorSetLayout tex_layout;
	CHECK(vkCreateDescriptorSetLayout(dev, &(VkDescriptorSetLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
		.bindingCount = 1,
		.pBindings = &(VkDescriptorSetLayoutBinding){
			0, VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER, 1, VK_SHADER_STAGE_FRAGMENT_BIT,
		},
	}, NULL, &tex_layout));
	VkDescriptorPool tex_pool;
	CHECK(vkCreateDescriptorPool(dev, &(VkDescriptorPoolCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
		.maxSets = 1,
		.poolSizeCount = 1,
		.pPoolSizes = &(VkDescriptorPoolSize){ VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER, 1 },
	}, NULL, &tex_pool));
	VkDescriptorSet tex_set;
	CHECK(vkAllocateDescriptorSets(dev, &(VkDescriptorSetAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
		.descriptorPool = tex_pool,
		.descriptorSetCount = 1,
		.pSetLayouts = &tex_layout,
	}, &tex_set));
	vkUpdateDescriptorSets(dev, 1, &(VkWriteDescriptorSet){
		.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
		.dstSet = tex_set,
		.descriptorCount = 1,
		.descriptorType = VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
		.pImageInfo = &(VkDescriptorImageInfo){
			sampler, tex_view, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
		},
	}, 0, NULL);

	VkPipelineLayout gfx_layout;
	CHECK(vkCreatePipelineLayout(dev, &(VkPipelineLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
		.setLayoutCount = tex ? 2 : ubo,
		.pSetLayouts = (VkDescriptorSetLayout[]){ ubo_layout, tex_layout },
	}, NULL, &gfx_layout));

	int vary = getenv("VARY") != NULL;
	int probe = getenv("PROBE") ? atoi(getenv("PROBE")) : -1;
	VkShaderModule vert = probe >= 0 ? shader(dev, probe_vert, sizeof(probe_vert)) :
			      vary ? shader(dev, tri_vary_vert, sizeof(tri_vary_vert)) :
			      ubo ? shader(dev, tri_ubo_vert, sizeof(tri_ubo_vert)) :
				    shader(dev, tri_vert, sizeof(tri_vert));
	int deriv = getenv("DERIV") != NULL;
	VkShaderModule frag = probe >= 0 ? shader(dev, probe_frag, sizeof(probe_frag)) :
			      deriv ? shader(dev, tri_deriv_frag, sizeof(tri_deriv_frag)) :
			      vary ? shader(dev, tri_vary_frag, sizeof(tri_vary_frag)) :
			      tex ? shader(dev, tri_tex_frag, sizeof(tri_tex_frag)) :
				    shader(dev, tri_frag, sizeof(tri_frag));
	VkPipeline gfx;
	CHECK(vkCreateGraphicsPipelines(dev, VK_NULL_HANDLE, 1, &(VkGraphicsPipelineCreateInfo){
		.sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
		.stageCount = 2,
		.pStages = (VkPipelineShaderStageCreateInfo[]){
			{
				.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
				.stage = VK_SHADER_STAGE_VERTEX_BIT,
				.module = vert,
				.pName = "main",
			},
			{
				.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
				.stage = VK_SHADER_STAGE_FRAGMENT_BIT,
				.module = frag,
				.pName = "main",
				.pSpecializationInfo = &(VkSpecializationInfo){
					.mapEntryCount = 1,
					.pMapEntries = &(VkSpecializationMapEntry){ 0, 0, sizeof(int) },
					.dataSize = sizeof(int),
					.pData = &probe,
				},
			},
		},
		.pVertexInputState = &(VkPipelineVertexInputStateCreateInfo){
			.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
		},
		.pInputAssemblyState = &(VkPipelineInputAssemblyStateCreateInfo){
			.sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
			.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
		},
		.pViewportState = &(VkPipelineViewportStateCreateInfo){
			.sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
			.viewportCount = 1,
			.pViewports = &(VkViewport){ vp_x, vp_y, vp_w, vp_h, 0, 1 },
			.scissorCount = 1,
			.pScissors = &(VkRect2D){ { vp_x, vp_y }, { vp_w, vp_h } },
		},
		.pRasterizationState = &(VkPipelineRasterizationStateCreateInfo){
			.sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
			.polygonMode = VK_POLYGON_MODE_FILL,
			.cullMode = VK_CULL_MODE_NONE,
			.lineWidth = 1.0f,
		},
		.pMultisampleState = &(VkPipelineMultisampleStateCreateInfo){
			.sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
			.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT,
		},
		.pDepthStencilState = &(VkPipelineDepthStencilStateCreateInfo){
			.sType = VK_STRUCTURE_TYPE_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO,
			.depthTestEnable = depth != 0,
			.depthWriteEnable = depth != 0,
			.depthCompareOp = VK_COMPARE_OP_LESS,
		},
		.pColorBlendState = &(VkPipelineColorBlendStateCreateInfo){
			.sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
			.attachmentCount = 1,
			.pAttachments = &(VkPipelineColorBlendAttachmentState){
				.colorWriteMask = 0xf,
			},
		},
		.layout = gfx_layout,
		.renderPass = pass,
	}, NULL, &gfx));

	VkDescriptorSetLayout set_layout;
	CHECK(vkCreateDescriptorSetLayout(dev, &(VkDescriptorSetLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
		.bindingCount = 2,
		.pBindings = (VkDescriptorSetLayoutBinding[]){
			{ 0, VK_DESCRIPTOR_TYPE_STORAGE_IMAGE, 1, VK_SHADER_STAGE_COMPUTE_BIT },
			{ 1, VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1, VK_SHADER_STAGE_COMPUTE_BIT },
		},
	}, NULL, &set_layout));
	VkPipelineLayout comp_layout;
	CHECK(vkCreatePipelineLayout(dev, &(VkPipelineLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
		.setLayoutCount = 1,
		.pSetLayouts = &set_layout,
	}, NULL, &comp_layout));
	VkShaderModule comp = shader(dev, read_comp, sizeof(read_comp));
	VkShaderModule fill = shader(dev, store_comp, sizeof(store_comp));
	VkPipeline pipes[2];
	CHECK(vkCreateComputePipelines(dev, VK_NULL_HANDLE, 2, (VkComputePipelineCreateInfo[]){
		{
			.sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
			.stage = {
				.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
				.stage = VK_SHADER_STAGE_COMPUTE_BIT,
				.module = comp,
				.pName = "main",
			},
			.layout = comp_layout,
		},
		{
			.sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
			.stage = {
				.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
				.stage = VK_SHADER_STAGE_COMPUTE_BIT,
				.module = fill,
				.pName = "main",
			},
			.layout = comp_layout,
		},
	}, NULL, pipes));
	VkPipeline readback = pipes[0], filler = pipes[1];

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
			.pBufferInfo = &(VkDescriptorBufferInfo){ buf, 0, size },
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
	if (store) {
		vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, filler);
		vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, comp_layout, 0, 1, &set,
					0, NULL);
		vkCmdDispatch(cmd, DIV_ROUND_UP(W, 8), DIV_ROUND_UP(H, 8), 1);
		vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
				     VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, 0, 1, &(VkMemoryBarrier){
					     .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
					     .srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT,
					     .dstAccessMask = VK_ACCESS_SHADER_READ_BIT,
				     }, 0, NULL, 0, NULL);
	} else {
	if (tex == 2) {
		VkImageSubresourceRange range = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1 };

		vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
				     VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, NULL, 0, NULL, 1,
				     &(VkImageMemoryBarrier){
					     .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
					     .dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT,
					     .oldLayout = VK_IMAGE_LAYOUT_UNDEFINED,
					     .newLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
					     .image = tex_image,
					     .subresourceRange = range,
				     });
		vkCmdCopyBufferToImage(cmd, staging, tex_image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
				       1, &(VkBufferImageCopy){
					       .imageSubresource = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1 },
					       .imageExtent = { 64, 64, 1 },
				       });
		vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TRANSFER_BIT,
				     VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 0, 0, NULL, 0, NULL, 1,
				     &(VkImageMemoryBarrier){
					     .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
					     .srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT,
					     .dstAccessMask = VK_ACCESS_SHADER_READ_BIT,
					     .oldLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
					     .newLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
					     .image = tex_image,
					     .subresourceRange = range,
				     });
	} else if (tex) {
		vkCmdBeginRenderPass(cmd, &(VkRenderPassBeginInfo){
			.sType = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
			.renderPass = tex_pass,
			.framebuffer = tex_fb,
			.renderArea = { { 0, 0 }, { 64, 64 } },
			.clearValueCount = 1,
			.pClearValues = &(VkClearValue){ .color.float32 = { 0, 1, 0, 1 } },
		}, VK_SUBPASS_CONTENTS_INLINE);
		vkCmdEndRenderPass(cmd);
		vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
				     VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 0, 1, &(VkMemoryBarrier){
					     .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
					     .srcAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
					     .dstAccessMask = VK_ACCESS_SHADER_READ_BIT,
				     }, 0, NULL, 0, NULL);
	}
	vkCmdBeginRenderPass(cmd, &(VkRenderPassBeginInfo){
		.sType = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
		.renderPass = pass,
		.framebuffer = fb,
		.renderArea = { { 0, 0 }, { W, H } },
		.clearValueCount = depth ? 2 : 1,
		.pClearValues = (VkClearValue[]){
			{ .color.float32 = { 0, 0, 1, 1 } },
			{ .depthStencil = { depth == 2 ? 0.0f : 1.0f, 0 } },
		},
	}, VK_SUBPASS_CONTENTS_INLINE);
	if (draw) {
		vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, gfx);
		if (ubo)
			vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, gfx_layout, 0, 1,
						&ubo_set, 0, NULL);
		if (tex)
			vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, gfx_layout, 1, 1,
						&tex_set, 0, NULL);
		vkCmdDraw(cmd, 3, 1, 0, 0);
	}
	vkCmdEndRenderPass(cmd);
	}
	vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
			     VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, 0, 1, &(VkMemoryBarrier){
				     .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
				     .srcAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
				     .dstAccessMask = VK_ACCESS_SHADER_READ_BIT,
			     }, 0, NULL, 0, NULL);
	vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, readback);
	vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, comp_layout, 0, 1, &set, 0, NULL);
	vkCmdDispatch(cmd, DIV_ROUND_UP(W, 8), DIV_ROUND_UP(H, 8), 1);
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

	uint32_t step = getenv("FULL") ? 1 : MAX(1, MAX(W, H) / 48);

	for (uint32_t y = 0; y < H; y += step * 2) {
		for (uint32_t x = 0; x < W; x += step) {
			uint32_t p = pixels[y * W + x];
			putchar(p == RED ? '#' : p == BLUE ? '.' : '?');
		}
		putchar('\n');
	}

	if (getenv("RAW")) {
		uint32_t *raw;
		uint32_t count = 0;

		CHECK(vkMapMemory(dev, image_mem, 0, image_size, 0, (void **)&raw));
		for (uint32_t i = 0; i < image_size / 4; i++) {
			if (!raw[i])
				continue;
			uint32_t j = i;
			while (j + 1 < image_size / 4 && raw[j + 1] == raw[i])
				j++;
			printf("raw[%d..%d] = 0x%08x\n", (int)(i - image_offset / 4),
			       (int)(j - image_offset / 4), raw[i]);
			count += j - i + 1;
			i = j;
		}
		printf("%u of %u raw words non-zero\n", count, (uint32_t)(image_size / 4));
		printf("raw corner 0x%08x, centre 0x%08x\n", raw[image_offset / 4],
		       raw[twiddle(W / 2, H / 2, W, H) + image_offset / 4]);

		for (uint32_t y = 0; y < H; y += step * 2) {
			for (uint32_t x = 0; x < W; x += step) {
				uint32_t m = twiddle(x, y, W, H) + image_offset / 4;
				putchar(raw[m] == RED ? '#' : raw[m] == BLUE ? '.' : ' ');
			}
			putchar('\n');
		}
	}

	if (getenv("HEX"))
		for (int i = 0; i < 64; i++)
			printf("%08x%c", pixels[i], i % 8 == 7 ? '\n' : ' ');

	printf("image size %ux%u\n", pixels[W * H], pixels[W * H + 1]);

	uint32_t corner = pixels[0];
	uint32_t centre = pixels[(H / 2) * W + W / 2];
	printf("corner 0x%08x, centre 0x%08x\n", corner, centre);

	uint32_t want_corner = store ? GREEN : BLUE;
	uint32_t want_centre = draw && depth != 2 ? (vary || deriv ? YELLOW : tex ? GREEN : RED) :
						    want_corner;
	int ok = corner == want_corner && centre == want_centre;
	printf("%s\n", ok ? "PASS" : "FAIL");

	if (getenv("PAUSE"))
		sleep(strtoul(getenv("PAUSE"), NULL, 0));

	vkDeviceWaitIdle(dev);
	vkDestroyDevice(dev, NULL);
	vkDestroyInstance(instance, NULL);

	return ok ? 0 : 1;
}
