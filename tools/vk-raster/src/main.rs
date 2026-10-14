//! Checks where the GPU samples each pixel: gl_FragCoord, varying interpolation and
//! nearest texture lookups of a texture whose texels hold their own coordinates.
//!
//! usage: [WIDTH=n] [HEIGHT=n] vk-raster

use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::CStr;

use ash::vk;

const VERT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/quad.vert.spv"));
const FRAG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/probe.frag.spv"));
const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const MODES: [&str; 7] = [
    "fragcoord", "varying", "texture", "texelfetch", "sub", "mod2", "centroid-sample",
];

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

struct Gpu {
    device: ash::Device,
    memory_props: vk::PhysicalDeviceMemoryProperties,
}

impl Gpu {
    fn memory_type(&self, bits: u32, want: vk::MemoryPropertyFlags) -> Result<u32> {
        (0..self.memory_props.memory_type_count)
            .find(|&i| {
                bits & (1 << i) != 0
                    && self.memory_props.memory_types[i as usize].property_flags.contains(want)
            })
            .ok_or_else(|| "no suitable memory type".into())
    }

    unsafe fn bind_image(&self, image: vk::Image) -> Result<vk::DeviceMemory> {
        let req = unsafe { self.device.get_image_memory_requirements(image) };
        let mem = self.allocate(req, vk::MemoryPropertyFlags::empty())?;
        unsafe { self.device.bind_image_memory(image, mem, 0)? };
        Ok(mem)
    }

    unsafe fn host_buffer(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> Result<(vk::Buffer, vk::DeviceMemory, *mut u8)> {
        let buffer = unsafe {
            self.device
                .create_buffer(&vk::BufferCreateInfo::default().size(size).usage(usage), None)?
        };
        let req = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let mem = self.allocate(
            req,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe {
            self.device.bind_buffer_memory(buffer, mem, 0)?;
            let ptr = self.device.map_memory(mem, 0, size, vk::MemoryMapFlags::empty())?;
            Ok((buffer, mem, ptr.cast()))
        }
    }

    fn allocate(
        &self,
        req: vk::MemoryRequirements,
        want: vk::MemoryPropertyFlags,
    ) -> Result<vk::DeviceMemory> {
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(self.memory_type(req.memory_type_bits, want)?);
        Ok(unsafe { self.device.allocate_memory(&info, None)? })
    }

    unsafe fn image(&self, w: u32, h: u32, usage: vk::ImageUsageFlags) -> Result<vk::Image> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(FORMAT)
            .extent(vk::Extent3D { width: w, height: h, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage);
        Ok(unsafe { self.device.create_image(&info, None)? })
    }

    unsafe fn view(&self, image: vk::Image) -> Result<vk::ImageView> {
        let info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(FORMAT)
            .subresource_range(color_range());
        Ok(unsafe { self.device.create_image_view(&info, None)? })
    }

    unsafe fn shader(&self, spv: &[u8]) -> Result<vk::ShaderModule> {
        let code = ash::util::read_spv(&mut std::io::Cursor::new(spv))?;
        let info = vk::ShaderModuleCreateInfo::default().code(&code);
        Ok(unsafe { self.device.create_shader_module(&info, None)? })
    }
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: 0,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn barrier(
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
    src: vk::AccessFlags,
    dst: vk::AccessFlags,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .image(image)
        .old_layout(old)
        .new_layout(new)
        .src_access_mask(src)
        .dst_access_mask(dst)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .subresource_range(color_range())
}

fn expected(mode: usize, x: u32, y: u32) -> [u8; 4] {
    match mode {
        0 => [128, 128, 0, 255],
        1 => [128, 128, 128, 128],
        4 | 6 => [128, 128, 128, 128],
        5 => {
            let q = |v: u32| if v % 2 == 0 { 64 } else { 191 };
            [q(x), q(y), q(x), q(y)]
        }
        _ => [x as u8, y as u8, 0, 255],
    }
}

fn describe(mode: usize, px: [u8; 4], x: u32, y: u32) -> String {
    match mode {
        0 => format!("frac {:.3},{:.3}", px[0] as f32 / 255.0, px[1] as f32 / 255.0),
        1 => format!(
            "frac {:.3},{:.3} floor-diff {:+},{:+}",
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            (px[2] as f32 * 16.0 / 255.0).round() as i32 - 8,
            (px[3] as f32 * 16.0 / 255.0).round() as i32 - 8
        ),
        4..=6 => format!(
            "{:.3},{:.3} {:.3},{:.3}",
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
            px[3] as f32 / 255.0
        ),
        _ => format!("texel {:+},{:+}", px[0] as i32 - x as i32, px[1] as i32 - y as i32),
    }
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(&a, b)| a.abs_diff(b) <= 1)
}

fn main() -> Result<()> {
    let w = env_u32("WIDTH", 64);
    let h = env_u32("HEIGHT", w);
    assert!(w <= 256 && h <= 256, "texels encode coordinates in 8 bits");

    unsafe { run(w, h) }
}

unsafe fn run(w: u32, h: u32) -> Result<()> {
    let entry = unsafe { ash::Entry::load()? };
    let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);
    let mut instance_exts = Vec::new();
    let mut instance_flags = vk::InstanceCreateFlags::empty();
    if unsafe { entry.enumerate_instance_extension_properties(None)? }
        .iter()
        .any(|e| e.extension_name_as_c_str() == Ok(c"VK_KHR_portability_enumeration"))
    {
        instance_exts.push(c"VK_KHR_portability_enumeration".as_ptr());
        instance_flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
    }
    let instance = unsafe {
        entry.create_instance(
            &vk::InstanceCreateInfo::default()
                .application_info(&app)
                .enabled_extension_names(&instance_exts)
                .flags(instance_flags),
            None,
        )?
    };
    let phys = unsafe { instance.enumerate_physical_devices()? }[0];
    let props = unsafe { instance.get_physical_device_properties(phys) };
    let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) };
    println!("device: {}, {w}x{h}", name.to_string_lossy());

    let family = unsafe { instance.get_physical_device_queue_family_properties(phys) }
        .iter()
        .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .ok_or("no graphics queue")? as u32;
    let prio = [1.0];
    let queue_info = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&prio)];
    let mut exts = Vec::new();
    if unsafe { instance.enumerate_device_extension_properties(phys)? }
        .iter()
        .any(|e| e.extension_name_as_c_str() == Ok(c"VK_KHR_portability_subset"))
    {
        exts.push(c"VK_KHR_portability_subset".as_ptr());
    }
    let supported = unsafe { instance.get_physical_device_features(phys) };
    let features =
        vk::PhysicalDeviceFeatures::default().sample_rate_shading(supported.sample_rate_shading != 0);
    let device = unsafe {
        instance.create_device(
            phys,
            &vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_info)
                .enabled_extension_names(&exts)
                .enabled_features(&features),
            None,
        )?
    };
    let queue = unsafe { device.get_device_queue(family, 0) };
    let gpu = Gpu {
        device: device.clone(),
        memory_props: unsafe { instance.get_physical_device_memory_properties(phys) },
    };

    let size = (w * h * 4) as u64;
    let target = unsafe {
        gpu.image(w, h, vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)?
    };
    unsafe { gpu.bind_image(target)? };
    let target_view = unsafe { gpu.view(target)? };
    let tex = unsafe {
        gpu.image(w, h, vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)?
    };
    unsafe { gpu.bind_image(tex)? };
    let tex_view = unsafe { gpu.view(tex)? };

    let (upload, _, upload_ptr) =
        unsafe { gpu.host_buffer(size, vk::BufferUsageFlags::TRANSFER_SRC)? };
    let texels = unsafe { std::slice::from_raw_parts_mut(upload_ptr, size as usize) };
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            texels[i..i + 4].copy_from_slice(&[x as u8, y as u8, 0, 255]);
        }
    }
    let (readback, _, readback_ptr) =
        unsafe { gpu.host_buffer(size, vk::BufferUsageFlags::TRANSFER_DST)? };

    let sampler = unsafe {
        device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            None,
        )?
    };
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
    let set_layout = unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?
    };
    let pool_sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 1,
    }];
    let pool = unsafe {
        device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default().max_sets(1).pool_sizes(&pool_sizes),
            None,
        )?
    };
    let layouts = [set_layout];
    let set = unsafe {
        device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts),
        )?
    }[0];
    let image_info = [vk::DescriptorImageInfo {
        sampler,
        image_view: tex_view,
        image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    }];
    unsafe {
        device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(set)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_info)],
            &[],
        )
    };

    let attachments = [vk::AttachmentDescription::default()
        .format(FORMAT)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)];
    let color_ref = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let subpass = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_ref)];
    let render_pass = unsafe {
        device.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpass),
            None,
        )?
    };
    let views = [target_view];
    let framebuffer = unsafe {
        device.create_framebuffer(
            &vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&views)
                .width(w)
                .height(h)
                .layers(1),
            None,
        )?
    };

    let push_range = [vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        offset: 0,
        size: 12,
    }];
    let pipeline_layout = unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&layouts)
                .push_constant_ranges(&push_range),
            None,
        )?
    };
    let vert = unsafe { gpu.shader(VERT)? };
    let frag = unsafe { gpu.shader(FRAG)? };
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(c"main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(c"main"),
    ];
    let viewport = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: w as f32,
        height: h as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }];
    let scissor = [vk::Rect2D { offset: vk::Offset2D::default(), extent: vk::Extent2D { width: w, height: h } }];
    let blend = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewport)
        .scissors(&scissor);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend);
    let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .layout(pipeline_layout)
        .render_pass(render_pass)];
    let pipeline = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
            .map_err(|(_, e)| e)?
    }[0];

    let cmd_pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?
    };
    let cmd = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(cmd_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?
    }[0];
    let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None)? };
    let submit = |record: &dyn Fn()| -> Result<()> {
        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
            record();
            device.end_command_buffer(cmd)?;
            let cmds = [cmd];
            device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)?;
            device.wait_for_fences(&[fence], true, 5_000_000_000)?;
            device.reset_fences(&[fence])?;
        }
        Ok(())
    };

    let region = [vk::BufferImageCopy {
        buffer_offset: 0,
        buffer_row_length: 0,
        buffer_image_height: 0,
        image_subresource: color_layers(),
        image_offset: vk::Offset3D::default(),
        image_extent: vk::Extent3D { width: w, height: h, depth: 1 },
    }];
    submit(&|| unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::HOST,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier(
                tex,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
            )],
        );
        device.cmd_copy_buffer_to_image(
            cmd,
            upload,
            tex,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &region,
        );
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier(
                tex,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
            )],
        );
    })?;

    let mut failed = false;
    for (mode, mode_name) in MODES.iter().enumerate() {
        let mut push = [0u8; 12];
        push[0..4].copy_from_slice(&(w as f32).to_ne_bytes());
        push[4..8].copy_from_slice(&(h as f32).to_ne_bytes());
        push[8..12].copy_from_slice(&(mode as i32).to_ne_bytes());
        let clear = [vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } }];
        submit(&|| unsafe {
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(render_pass)
                    .framebuffer(framebuffer)
                    .render_area(scissor[0])
                    .clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                0,
                &[set],
                &[],
            );
            device.cmd_push_constants(
                cmd,
                pipeline_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &push,
            );
            device.cmd_draw(cmd, 6, 1, 0, 0);
            device.cmd_end_render_pass(cmd);
            device.cmd_copy_image_to_buffer(
                cmd,
                target,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback,
                &region,
            );
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
        })?;

        let pixels = unsafe { std::slice::from_raw_parts(readback_ptr, size as usize) };
        let mut wrong = BTreeMap::<String, (u32, u32, u32)>::new();
        let mut map = String::new();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let px = [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]];
                let ok = close(px, expected(mode, x, y));
                if !ok {
                    let e = wrong.entry(describe(mode, px, x, y)).or_insert((0, x, y));
                    e.0 += 1;
                }
                if y < 16 && x < 32 {
                    map.push(if ok { '.' } else { '#' });
                }
            }
            if y < 16 {
                map.push('\n');
            }
        }
        let bad: u32 = wrong.values().map(|v| v.0).sum();
        println!("== {mode_name}: {} of {} pixels correct", w * h - bad, w * h);
        if bad > 0 {
            failed = true;
            let mut kinds: Vec<_> = wrong.into_iter().collect();
            kinds.sort_by_key(|k| std::cmp::Reverse(k.1.0));
            for (what, (count, x, y)) in kinds.iter().take(8) {
                println!("  {count:5} x {what} (first at {x},{y})");
            }
            print!("top-left 32x16 (# = wrong):\n{map}");
        }
    }

    if failed {
        std::process::exit(1);
    }
    Ok(())
}
