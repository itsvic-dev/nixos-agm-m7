// usage: vk-compute [groups]
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <vulkan/vulkan.h>

#include "fill.spv.h"

#define CHECK(expr)                                                         \
	do {                                                                \
		VkResult r_ = (expr);                                       \
		if (r_ != VK_SUCCESS) {                                     \
			fprintf(stderr, "%s failed: %d\n", #expr, r_);      \
			return 1;                                           \
		}                                                           \
	} while (0)

int main(int argc, char **argv)
{
	uint32_t groups = argc > 1 ? strtoul(argv[1], NULL, 0) : 1;
	uint32_t count = groups * 64;
	VkDeviceSize size = count * sizeof(uint32_t);

	VkInstanceCreateInfo instance_info = {
		.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
		.pApplicationInfo = &(VkApplicationInfo){
			.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
			.apiVersion = VK_API_VERSION_1_2,
		},
	};
	VkInstance instance;
	CHECK(vkCreateInstance(&instance_info, NULL, &instance));

	uint32_t n = 1;
	VkPhysicalDevice phys;
	CHECK(vkEnumeratePhysicalDevices(instance, &n, &phys));

	VkPhysicalDeviceProperties props;
	vkGetPhysicalDeviceProperties(phys, &props);
	printf("device: %s\n", props.deviceName);

	uint32_t qf_count = 8;
	VkQueueFamilyProperties qf[8];
	vkGetPhysicalDeviceQueueFamilyProperties(phys, &qf_count, qf);
	uint32_t family = 0;
	while (family < qf_count && !(qf[family].queueFlags & VK_QUEUE_COMPUTE_BIT))
		family++;

	float prio = 1.0f;
	VkDeviceCreateInfo device_info = {
		.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
		.queueCreateInfoCount = 1,
		.pQueueCreateInfos = &(VkDeviceQueueCreateInfo){
			.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
			.queueFamilyIndex = family,
			.queueCount = 1,
			.pQueuePriorities = &prio,
		},
	};
	VkDevice dev;
	CHECK(vkCreateDevice(phys, &device_info, NULL, &dev));
	VkQueue queue;
	vkGetDeviceQueue(dev, family, 0, &queue);

	VkBuffer buf;
	CHECK(vkCreateBuffer(dev, &(VkBufferCreateInfo){
		.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		.size = size,
		.usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT,
	}, NULL, &buf));

	VkMemoryRequirements req;
	vkGetBufferMemoryRequirements(dev, buf, &req);
	VkPhysicalDeviceMemoryProperties mem_props;
	vkGetPhysicalDeviceMemoryProperties(phys, &mem_props);
	uint32_t type = 0;
	VkMemoryPropertyFlags want = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
				     VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
	while (type < mem_props.memoryTypeCount &&
	       (!(req.memoryTypeBits & (1u << type)) ||
		(mem_props.memoryTypes[type].propertyFlags & want) != want))
		type++;

	VkDeviceMemory mem;
	CHECK(vkAllocateMemory(dev, &(VkMemoryAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = req.size,
		.memoryTypeIndex = type,
	}, NULL, &mem));
	CHECK(vkBindBufferMemory(dev, buf, mem, 0));

	uint32_t *data;
	CHECK(vkMapMemory(dev, mem, 0, size, 0, (void **)&data));
	memset(data, 0xaa, size);

	VkShaderModule shader;
	CHECK(vkCreateShaderModule(dev, &(VkShaderModuleCreateInfo){
		.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
		.codeSize = sizeof(fill_spv),
		.pCode = fill_spv,
	}, NULL, &shader));

	VkDescriptorSetLayoutBinding binding = {
		.binding = 0,
		.descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
		.descriptorCount = 1,
		.stageFlags = VK_SHADER_STAGE_COMPUTE_BIT,
	};
	VkDescriptorSetLayout set_layout;
	CHECK(vkCreateDescriptorSetLayout(dev, &(VkDescriptorSetLayoutCreateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
		.bindingCount = 1,
		.pBindings = &binding,
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
		.poolSizeCount = 1,
		.pPoolSizes = &(VkDescriptorPoolSize){ VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1 },
	}, NULL, &pool));

	VkDescriptorSet set;
	CHECK(vkAllocateDescriptorSets(dev, &(VkDescriptorSetAllocateInfo){
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
		.descriptorPool = pool,
		.descriptorSetCount = 1,
		.pSetLayouts = &set_layout,
	}, &set));
	vkUpdateDescriptorSets(dev, 1, &(VkWriteDescriptorSet){
		.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
		.dstSet = set,
		.descriptorCount = 1,
		.descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
		.pBufferInfo = &(VkDescriptorBufferInfo){ buf, 0, size },
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
	vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline);
	vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, layout, 0, 1, &set, 0, NULL);
	vkCmdDispatch(cmd, groups, 1, 1);
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
	for (uint32_t i = 0; i < count; i++) {
		if (data[i] != i * 3 + 1) {
			if (bad++ < 8)
				printf("values[%u] = 0x%08x, expected 0x%08x\n", i, data[i], i * 3 + 1);
		}
	}
	printf("%u of %u values correct\n", count - bad, count);

	vkDestroyFence(dev, fence, NULL);
	vkDestroyCommandPool(dev, cmd_pool, NULL);
	vkDestroyDescriptorPool(dev, pool, NULL);
	vkDestroyPipeline(dev, pipeline, NULL);
	vkDestroyPipelineLayout(dev, layout, NULL);
	vkDestroyDescriptorSetLayout(dev, set_layout, NULL);
	vkDestroyShaderModule(dev, shader, NULL);
	vkUnmapMemory(dev, mem);
	vkFreeMemory(dev, mem, NULL);
	vkDestroyBuffer(dev, buf, NULL);
	vkDestroyDevice(dev, NULL);
	vkDestroyInstance(instance, NULL);

	return bad ? 1 : 0;
}
