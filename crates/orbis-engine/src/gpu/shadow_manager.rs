use wgpu::{BindGroup, BindGroupLayout, Buffer, Device, RenderPipeline, util::DeviceExt};

use crate::{
    ResourceId,
    assets::{
        self,
        texture_asset::{ColorSpace, SamplerDesc},
    },
    gpu::{
        Dimension::Array, GpuContextRef, GpuResourceStats, GpuTexture, GpuTextureBuilder,
        GpuTextureUsage, HasGpuStats, pipeline_manager::PipelineExt,
    },
    renderer::uniform::LightUniform,
};

const MAX_SHADOWS: usize = 64;
const SHADOWS_SIZE: u32 = 1024;

impl HasGpuStats for ShadowManager {
    fn get_stats(&self) -> GpuResourceStats {
        self.stats.clone()
    }
}

pub struct ShadowManager {
    texture_rgba_id: ResourceId,
    texture_rgba: GpuTexture,
    buffer: wgpu::Buffer,
    pipeline: RenderPipeline,
    bindgroup: BindGroup,
    shadow_map: GpuTexture, //max layer = MAX_SHADOWS
    layer_views: Vec<wgpu::TextureView>,
    #[allow(dead_code)]
    stats: GpuResourceStats,
}

impl ShadowManager {
    pub fn new(gpu: &GpuContextRef) -> Self {
        let shadow_map = GpuTextureBuilder::from_empty(SHADOWS_SIZE, SHADOWS_SIZE)
            .format(ColorSpace::Depth32f)
            .usage(GpuTextureUsage::SampledTexture)
            .dimension(Array(64))
            .sampler(SamplerDesc::DepthComparison)
            .label("shadow_texture depth array")
            .build(gpu);

        let layer_views = (0..MAX_SHADOWS as u32)
            .map(|layer| {
                shadow_map.inner.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();

        // debug rgba8 texture
        let texture_rgba = GpuTextureBuilder::from_empty(SHADOWS_SIZE, SHADOWS_SIZE)
            .format(ColorSpace::Rgba8)
            .usage(GpuTextureUsage::SampledTexture)
            .sampler(SamplerDesc::NearestClamp)
            .label("shadow_texture rgba")
            .build(gpu);

        let stats = GpuResourceStats {
            count: MAX_SHADOWS,
            estimated_bytes: shadow_map.estimated_size,
        };

        let (buffer, bindgroup, pipeline) = {
            let layout = Self::create_layout(gpu.device);
            let buffer = Self::create_buffer(gpu.device);
            let bindgroup = Self::create_bg(gpu.device, &buffer, &layout);
            let pipeline = Self::create_pipeline(gpu.device, &layout);
            (buffer, bindgroup, pipeline)
        };

        Self {
            texture_rgba_id: ResourceId::new(),
            shadow_map,
            layer_views,
            stats,
            texture_rgba,
            buffer,
            bindgroup,
            pipeline,
        }
    }
}

impl ShadowManager {
    pub fn get_shadowmap_view(&self, slot: usize) -> Option<&wgpu::TextureView> {
        self.layer_views.get(slot)
    }

    pub fn get_sampler(&self) -> &wgpu::Sampler {
        &self.shadow_map.sampler
    }

    pub fn get_views(&self) -> &wgpu::TextureView {
        &self.shadow_map.view
    }

    pub fn get_rgba(&self) -> &GpuTexture {
        &self.texture_rgba
    }

    pub fn get_rgba_id(&self) -> ResourceId {
        self.texture_rgba_id
    }

    pub fn get_bg(&self) -> &BindGroup {
        &self.bindgroup
    }

    pub fn get_buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn get_pipeline(&self) -> &RenderPipeline {
        &self.pipeline
    }
}

impl ShadowManager {
    fn create_buffer(device: &Device) -> Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light Uniform Buffer"),
            contents: bytemuck::cast_slice(&[LightUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn create_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ShadowMapCreate_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                // Ligth
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }
    fn create_bg(device: &Device, uniform_buffer: &Buffer, layout: &BindGroupLayout) -> BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                // Light
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
            label: Some("ShadowMapCreate_bind_group"),
        })
    }

    fn create_pipeline(device: &Device, layout: &BindGroupLayout) -> RenderPipeline {
        let layouts = [
            Some(layout), //0
            None,         //1
            None,         //2
            None,         //3
        ];
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ShadowMapCreate Pipeline Layout"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/shadow_map.wgsl"));
        let buffer_desc = &[
            assets::MeshVertexData::get_layout(),
            assets::VertexInstance::get_layout(),
        ];

        let pipeline_desc = PipelineExt {
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
                stencil: Default::default(),
            }),
            ..Default::default()
        };

        let target = &[];

        pipeline_desc.build_pipeline(
            "ShadowMap Pipeline",
            device,
            Some(&render_pipeline_layout),
            target,
            shader,
            buffer_desc,
        )
    }
}
