use super::*;

use wgpu::{TextureViewDescriptor, util::DeviceExt};
pub struct BRDFLUTBuilder {}

impl BRDFLUTBuilder {
    pub const TEXTURE_SIZE: u32 = 512;
    pub fn build(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
        let format = wgpu::TextureFormat::Rg16Float;
        let shader = device.create_shader_module(wgpu::include_wgsl!("../shaders/brdflut.wgsl"));
        let pipeline = utils::create_pipeline(device, format, shader, "BRDFLUT Pipeline");
        let size = Self::TEXTURE_SIZE;
        let mip_level_count = 1;
        let depth_or_array_layers = 1;

        let dest_texture = utils::create_texture(
            device,
            "BRDF_LUT",
            size,
            size,
            depth_or_array_layers,
            mip_level_count,
            format,
        );
        let dest_view = dest_texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&Default::default());
        Self::render_to_texture(&mut encoder, &pipeline, &dest_view);

        queue.submit([encoder.finish()]);

        dest_texture
    }

    fn render_to_texture(
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        frame_view: &wgpu::TextureView,
    ) {
        let clear_color = wgpu::Color::BLACK;

        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        renderpass.set_pipeline(pipeline);
        renderpass.draw(0..6, 0..1);
    }
}

struct PrefilerMapResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
    roughness_buffer: wgpu::Buffer,
}

impl PrefilerMapResources {
    fn new(
        device: &wgpu::Device,
        hdr_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self {
        let camera_buffer = utils::create_camera_buffer(device);
        let roughness_buffer = Self::create_roughness_buffer(device);
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/prefilter_map.wgsl"));
        let pipeline = utils::create_pipeline(device, format, shader, "Prefilter Pipeline");

        let layout = pipeline.get_bind_group_layout(0);
        let bind_group =
            Self::create_bind_group(device, hdr_view, &camera_buffer, &roughness_buffer, &layout);

        Self {
            pipeline,
            bind_group,
            camera_buffer,
            roughness_buffer,
        }
    }

    fn create_roughness_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Roughness Uniform Buffer"),
            contents: bytemuck::cast_slice(&[f32::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        camera_buffer: &wgpu::Buffer,
        roughness_buffer: &wgpu::Buffer,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PrefilterMap_bind_group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: roughness_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

pub struct PrefilterMap {}

impl PrefilterMap {
    pub const TEXTURE_SIZE: u32 = 128;
    pub const MIP_LEVELS: u32 = 8;

    pub fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cube_texture: &wgpu::Texture,
    ) -> wgpu::Texture {
        let format = wgpu::TextureFormat::Rgba16Float;

        let cube_texture_view = cube_texture.create_view(&TextureViewDescriptor {
            label: Some("Cube texture view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let target_texture = utils::create_cube_texture(
            device,
            "Prefilter_map",
            Self::TEXTURE_SIZE,
            Self::MIP_LEVELS,
            format,
        );

        render_cubemap_with_resources::<PrefilerMapResources>(
            device,
            queue,
            &cube_texture_view,
            &target_texture,
            Self::TEXTURE_SIZE,
            Self::MIP_LEVELS,
            format,
        );

        target_texture
    }
}

struct EquirectResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
}

impl EquirectResources {
    fn new(
        device: &wgpu::Device,
        hdr_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self {
        let camera_buffer = utils::create_camera_buffer(device);
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../shaders/equirectangular_to_cubemap.wgsl"
        ));
        let pipeline = utils::create_pipeline(device, format, shader, "Equirect Pipeline");
        let layout = pipeline.get_bind_group_layout(0);
        let bind_group = Self::create_bind_group(device, hdr_view, &camera_buffer, &layout);

        Self {
            pipeline,
            bind_group,
            camera_buffer,
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        unifomrm_buffer: &wgpu::Buffer,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: unifomrm_buffer.as_entire_binding(),
                },
            ],
            label: Some("equirect_bind_group"),
        })
    }
}

pub struct EquirectangularToCubemap {}

impl EquirectangularToCubemap {
    pub fn build(
        hdr_texture: &GpuTexture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cube_size: u32,
    ) -> wgpu::Texture {
        let dest_format = wgpu::TextureFormat::Rgba16Float;
        let mip_level_count = utils::mip_levels(cube_size);

        // create dest: HDR cubemap texture
        let target_texture = utils::create_cube_texture(
            device,
            "Hdr_Cube",
            cube_size,
            mip_level_count,
            dest_format,
        );

        render_cubemap_with_resources::<EquirectResources>(
            device,
            queue,
            &hdr_texture.view,
            &target_texture,
            cube_size,
            mip_level_count,
            dest_format,
        );

        target_texture
    }
}

struct IrradianceResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
}

impl IrradianceResources {
    fn new(
        device: &wgpu::Device,
        hdr_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self {
        let camera_buffer = utils::create_camera_buffer(device);
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../shaders/irradiance_convolution.wgsl"
        ));
        let pipeline = utils::create_pipeline(device, format, shader, "Irradiance Pipeline");
        let layout = pipeline.get_bind_group_layout(0);
        let bind_group = Self::create_bind_group(device, hdr_view, &camera_buffer, &layout);

        Self {
            pipeline,
            bind_group,
            camera_buffer,
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        unifomrm_buffer: &wgpu::Buffer,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: unifomrm_buffer.as_entire_binding(),
                },
            ],
            label: Some("Irradiance_bind_group"),
        })
    }
}

pub struct IrrarianceMap {}

impl IrrarianceMap {
    pub const TEXTURE_SIZE: u32 = 32;

    pub fn build(
        cube_texture: &wgpu::Texture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> wgpu::Texture {
        let dest_format = wgpu::TextureFormat::Rgba16Float;
        let mip_level_count = 1;

        let cube_texture_view = cube_texture.create_view(&TextureViewDescriptor {
            label: Some("Cube texture view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let target_texture = utils::create_cube_texture(
            device,
            "Irradiance_Cube",
            Self::TEXTURE_SIZE,
            mip_level_count,
            dest_format,
        );

        render_cubemap_with_resources::<IrradianceResources>(
            device,
            queue,
            &cube_texture_view,
            &target_texture,
            Self::TEXTURE_SIZE,
            mip_level_count,
            dest_format,
        );

        target_texture
    }
}

trait CubemapBuilderResources {
    fn new(
        device: &wgpu::Device,
        src_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self;
    fn pipeline(&self) -> &wgpu::RenderPipeline;
    fn bind_group(&self) -> &wgpu::BindGroup;
    fn camera_buffer(&self) -> &wgpu::Buffer;
    // Optional: for resources that need per-mip updates (like roughness)
    fn update_per_mip(&self, _queue: &wgpu::Queue, _mip_level: u32, _mip_level_count: u32) {
        // Default: do nothing
    }
}

impl CubemapBuilderResources for IrradianceResources {
    fn new(
        device: &wgpu::Device,
        src_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self {
        IrradianceResources::new(device, src_view, format)
    }
    fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
    fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
    fn camera_buffer(&self) -> &wgpu::Buffer {
        &self.camera_buffer
    }
}

impl CubemapBuilderResources for EquirectResources {
    fn new(
        device: &wgpu::Device,
        src_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self {
        EquirectResources::new(device, src_view, format)
    }
    fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
    fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
    fn camera_buffer(&self) -> &wgpu::Buffer {
        &self.camera_buffer
    }
}

impl CubemapBuilderResources for PrefilerMapResources {
    fn new(
        device: &wgpu::Device,
        src_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self {
        PrefilerMapResources::new(device, src_view, format)
    }
    fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
    fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
    fn camera_buffer(&self) -> &wgpu::Buffer {
        &self.camera_buffer
    }
    fn update_per_mip(&self, queue: &wgpu::Queue, mip_level: u32, mip_level_count: u32) {
        // Update roughness buffer for each mip level
        let roughness = if mip_level_count > 1 {
            mip_level as f32 / (mip_level_count - 1) as f32
        } else {
            0.0
        };
        queue.write_buffer(&self.roughness_buffer, 0, bytemuck::bytes_of(&roughness));
    }
}

// Generic function to render a cubemap using a resource type
fn render_cubemap_with_resources<R: CubemapBuilderResources>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src_view: &wgpu::TextureView,
    target_texture: &wgpu::Texture,
    cube_size: u32,
    mip_level_count: u32,
    format: wgpu::TextureFormat,
) {
    let resources = R::new(device, src_view, format);
    let camera_views = utils::create_camera_views();

    for mip_level in 0..mip_level_count {
        let capture_size = utils::mip_size(cube_size, mip_level);

        resources.update_per_mip(queue, mip_level, mip_level_count);

        camera_views.iter().enumerate().for_each(|(i, view)| {
            let mut encoder = device.create_command_encoder(&Default::default());
            utils::update_camera_buffer(queue, resources.camera_buffer(), *view);

            let dest_view = utils::create_dest_view(target_texture, i as u32, mip_level);

            utils::render_to_cubemap(
                &mut encoder,
                resources.pipeline(),
                resources.bind_group(),
                &dest_view,
                capture_size,
            );
            queue.submit([encoder.finish()]);
        });
    }
}
/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assets::asset_manager::AssetManager, test_utils};

    const HDR_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/core/newport_loft.hdr");
    const CUBEMAP_SIZE: u32 = 512;

    /// BRDFLut
    #[test]
    fn should_create_brdflut_rg16f_texture() {
        let (device, queue) = test_utils::get_device_and_queue();

        let brdflut = BRDFLUTBuilder::build(device, queue);

        assert_eq!(brdflut.format(), wgpu::TextureFormat::Rg16Float);
        assert_eq!(brdflut.width(), BRDFLUTBuilder::TEXTURE_SIZE);
        assert_eq!(brdflut.height(), BRDFLUTBuilder::TEXTURE_SIZE);
        assert_eq!(brdflut.mip_level_count(), 1); // <- only 1 mip level
        assert_eq!(brdflut.depth_or_array_layers(), 1); // <- 2D texture
        assert_eq!(brdflut.dimension(), wgpu::TextureDimension::D2);

        #[cfg(feature = "save_tests")]
        {
            test_utils::save_texture(&device, &queue, "brdflut.png", &brdflut, 0).unwrap();
        }
    }

    /// EquirectangularToCubemap
    #[test]
    fn should_crate_cubetexture_rgba16f_from_equirectangular() {
        let (device, queue) = test_utils::get_device_and_queue();
        let mut texture_cache = GpuTextureCache::new(device, queue);
        let mut asset_mgr = AssetManager::default();
        let hdr_id = asset_mgr
            .textures
            .from_file(HDR_PATH, TextureUsage::HDR16);

        asset_mgr.textures.load_cpu_textures();
        texture_cache.upload_textures(&mut asset_mgr.textures, device, queue);

        let hdr = texture_cache.get_or_fallback_white(hdr_id /* device, queue */);

        let cubemap = EquirectangularToCubemap::build(&hdr, &device, &queue, CUBEMAP_SIZE);

        assert_eq!(cubemap.format(), wgpu::TextureFormat::Rgba16Float);
        assert_eq!(cubemap.height(), CUBEMAP_SIZE);
        assert_eq!(cubemap.width(), CUBEMAP_SIZE);
        assert_eq!(cubemap.mip_level_count(), utils::mip_levels(CUBEMAP_SIZE));
        assert_eq!(cubemap.depth_or_array_layers(), 6); // <- cubemap
        assert_eq!(cubemap.dimension(), wgpu::TextureDimension::D2);

        // +X right, -X left, +Y top, -Y bottom, +Z front, -Z back
        #[cfg(feature = "save_tests")]
        {
            test_utils::save_cubemap_cross(&device, &queue, "cubemap.png", &cubemap).unwrap();
        }
    }

    /// PrefilterMap
    #[test]
    fn should_create_prefilter_rgba16f_cubemap() {
        let (device, queue) = test_utils::get_device_and_queue();
        let mut texture_cache = GpuTextureCache::new(device, queue);
        let mut asset_mgr = AssetManager::default();
        let hdr_id = asset_mgr
            .textures
            .from_file(HDR_PATH, crate::assets::TextureUsage::HDR16);

        asset_mgr.textures.load_cpu_textures();
        texture_cache.upload_textures(&mut asset_mgr.textures, device, queue);

        let hdr = texture_cache.get_or_fallback_white(hdr_id /* device, queue */);

        let cubemap = EquirectangularToCubemap::build(&hdr, &device, &queue, CUBEMAP_SIZE);

        let prefilter = PrefilterMap::build(device, queue, &cubemap);

        assert_eq!(prefilter.format(), wgpu::TextureFormat::Rgba16Float);
        assert_eq!(prefilter.height(), PrefilterMap::TEXTURE_SIZE);
        assert_eq!(prefilter.width(), PrefilterMap::TEXTURE_SIZE);
        assert_eq!(prefilter.mip_level_count(), PrefilterMap::MIP_LEVELS);
        assert_eq!(prefilter.depth_or_array_layers(), 6); // <- cubemap
        assert_eq!(prefilter.dimension(), wgpu::TextureDimension::D2);

        #[cfg(feature = "save_tests")]
        {
            test_utils::save_cubemap_cross(&device, &queue, "prefilter.png", &prefilter).unwrap();
        }
    }

    /// IrradianceCubemap
    #[test]
    fn should_crate_irradiance_cubetexture_rgba16f() {
        let (device, queue) = test_utils::get_device_and_queue();
        let mut texture_cache = GpuTextureCache::new(device, queue);
        let mut asset_mgr = AssetManager::default();
        let hdr_id = asset_mgr
            .textures
            .from_file(HDR_PATH, TextureUsage::HDR16);

        asset_mgr.textures.load_cpu_textures();
        texture_cache.upload_textures(&mut asset_mgr.textures, device, queue);

        let hdr = texture_cache.get_or_fallback_white(hdr_id /* device, queue */);

        let cubemap = EquirectangularToCubemap::build(&hdr, &device, &queue, CUBEMAP_SIZE);

        let irradiance = IrrarianceMap::build(&cubemap, &device, &queue);

        assert_eq!(irradiance.format(), wgpu::TextureFormat::Rgba16Float);
        assert_eq!(irradiance.width(), IrrarianceMap::TEXTURE_SIZE);
        assert_eq!(irradiance.height(), IrrarianceMap::TEXTURE_SIZE);
        assert_eq!(irradiance.mip_level_count(), 1); // <- only 1 mip level
        assert_eq!(irradiance.depth_or_array_layers(), 6); // <- cubemap
        assert_eq!(irradiance.dimension(), wgpu::TextureDimension::D2);

        #[cfg(feature = "save_tests")]
        {
            test_utils::save_cubemap_cross(&device, &queue, "Irradiance.png", &irradiance).unwrap();
        }
    }
}
 */
