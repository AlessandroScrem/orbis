use super::*;

use crate::assets;

use wgpu::DepthStencilState;

/// A description of a render pipeline.
/// Note: You can call `default()` to get a base implementation.
#[derive(Debug, Hash, Clone)]
pub struct PipelineDesc {
    pub primitive: wgpu::PrimitiveState,
    pub multisample: wgpu::MultisampleState,
    pub depth_stencil: Option<DepthStencilState>,
    pub blend: Option<wgpu::BlendState>,
}
impl Default for PipelineDesc {
    fn default() -> Self {
        Self {
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float, // o quello che hai usato per creare la texture
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            blend: Some(wgpu::BlendState::REPLACE),
        }
    }
}
impl PipelineDesc {
    pub fn build_pipeline(
        self,
        label: &str,
        device: &wgpu::Device,
        layout: Option<&wgpu::PipelineLayout>,
        format: wgpu::TextureFormat,
        shader: wgpu::ShaderModule,
        buffers: &[wgpu::VertexBufferLayout<'static>],
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: self.blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: self.primitive,
            multisample: self.multisample,
            depth_stencil: self.depth_stencil,
            multiview_mask: None,
            cache: None,
        })
    }
}

#[derive(Debug, Hash, Clone)]
pub struct PipelineExt {
    pub primitive: wgpu::PrimitiveState,
    pub multisample: wgpu::MultisampleState,
    pub depth_stencil: Option<DepthStencilState>,
}
impl Default for PipelineExt {
    fn default() -> Self {
        Self {
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float, // o quello che hai usato per creare la texture
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
        }
    }
}

impl PipelineExt {
    pub fn build_pipeline(
        self,
        label: &str,
        device: &wgpu::Device,
        layout: Option<&wgpu::PipelineLayout>,
        targets: &[Option<wgpu::ColorTargetState>],
        shader: wgpu::ShaderModule,
        buffers: &[wgpu::VertexBufferLayout<'static>],
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: self.primitive,
            multisample: self.multisample,
            depth_stencil: self.depth_stencil,
            multiview_mask: None,
            cache: None,
        })
    }
}

use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Debug, Clone, Copy, EnumIter)]
pub enum PipelineKind {
    BlinnPhong,
    Lines,
    Pbr,
    Hdr,
    LightIcon,
    Skybox,
    Outline,
    BuildMipmaps,
    Convert,
    // ShadowMap,
}

#[derive(Debug, Clone, Copy, EnumIter)]
pub enum CsPipelineKind {
    BuildMipmaps,
    CopyToMip0,
}

pub struct PipelineManager {
    pipelines: Vec<wgpu::RenderPipeline>,
    cs_pipelines: Vec<wgpu::ComputePipeline>,
}

impl PipelineManager {
    const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

    pub fn new(
        device: &wgpu::Device,
        gpu_resource_manager: &GpuManager,
        final_format: wgpu::TextureFormat,
    ) -> Self {
        let pipelines: Vec<wgpu::RenderPipeline> = PipelineKind::iter()
            .map(|kind| {
                create_pipeline(
                    device,
                    gpu_resource_manager,
                    kind,
                    Self::HDR_FORMAT,
                    final_format,
                )
            })
            .collect();

        let cs_pipelines: Vec<wgpu::ComputePipeline> = CsPipelineKind::iter()
            .map(|kind| create_cs_pipeline(device, gpu_resource_manager, kind))
            .collect();

        Self {
            pipelines,
            cs_pipelines,
        }
    }

    pub fn get_render_pipeline(&self, kind: PipelineKind) -> &wgpu::RenderPipeline {
        &self.pipelines[kind as usize]
    }
    pub fn get_compute_pipeline(&self, kind: CsPipelineKind) -> &wgpu::ComputePipeline {
        &self.cs_pipelines[kind as usize]
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    gpu_resource_manager: &GpuManager,
    kind: PipelineKind,
    hdr_format: wgpu::TextureFormat,
    final_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    match kind {
        PipelineKind::BlinnPhong => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::PerFrame)), //0
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Material)), //1
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Model)),    //2
            ];
            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("BlinnPhong Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });
            let shader =
                device.create_shader_module(wgpu::include_wgsl!("shaders/blinn_phong.wgsl"));
            let buffer_desc = &[assets::MeshVertexData::get_layout()];

            let pipeline_desc = PipelineDesc::default();

            pipeline_desc.build_pipeline(
                "BlinnPhong Pipeline",
                device,
                Some(&render_pipeline_layout),
                hdr_format,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::Lines => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Camera)), //0
            ];
            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Lines Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });
            let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/lines.wgsl"));
            let buffer_desc = &[assets::LinesVertexData::get_layout()];

            let pipeline_desc = PipelineDesc {
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::LineList,
                    ..Default::default()
                },
                depth_stencil: None,
                ..Default::default()
            };

            pipeline_desc.build_pipeline(
                "Lines Pipeline",
                device,
                Some(&render_pipeline_layout),
                hdr_format,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::Pbr => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::PerFrame)), //0
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Material)), //1
                None,                                                                           //2
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::PbrMaps)),  //3
            ];
            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Pbr Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });
            let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/pbr.wgsl"));
            let buffer_desc = &[
                assets::MeshVertexData::get_layout(),
                assets::VertexInstance::get_layout(),
            ];

            let targets = &[
                // 0:
                Some(wgpu::ColorTargetState {
                    format: hdr_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                // 1:
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rg32Uint,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ];

            let pipeline_desc = PipelineExt::default();

            pipeline_desc.build_pipeline(
                "Pbr Pipeline",
                device,
                Some(&render_pipeline_layout),
                targets,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::Hdr => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Hdr)), //0
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::PerFrame)), //1
            ];
            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Hdr Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });
            let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/hdr.wgsl"));
            let buffer_desc = &[];

            let pipeline_desc = PipelineDesc {
                depth_stencil: None,
                ..Default::default()
            };

            pipeline_desc.build_pipeline(
                "Hdr Pipeline",
                device,
                Some(&render_pipeline_layout),
                final_format,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::LightIcon => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::PerFrame)), //0
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::LightIcon)), //1
            ];

            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Light Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });

            let shader =
                device.create_shader_module(wgpu::include_wgsl!("shaders/light_icon.wgsl"));

            let buffer_desc = &[];

            let targets = &[
                // 0:
                Some(wgpu::ColorTargetState {
                    format: hdr_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                // 1:
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rg32Uint,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ];
            let pipeline_desc = PipelineExt::default();

            pipeline_desc.build_pipeline(
                "Light Pipeline",
                device,
                Some(&render_pipeline_layout),
                targets,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::Skybox => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::PerFrame)), //0
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Skybox)),   //1
            ];

            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Skybox Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });

            let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/skybox.wgsl"));

            let buffer_desc = &[];

            let pipeline_desc = PipelineDesc {
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                ..Default::default()
            };

            pipeline_desc.build_pipeline(
                "Skybox Pipeline",
                device,
                Some(&render_pipeline_layout),
                hdr_format,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::Outline => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::EntityId)), //0
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::PerFrame)), //1
            ];

            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Outline Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });

            let shader =
                device.create_shader_module(wgpu::include_wgsl!("shaders/outline_selection.wgsl"));

            let buffer_desc = &[];

            let pipeline_desc = PipelineDesc {
                depth_stencil: None,
                ..Default::default()
            };

            pipeline_desc.build_pipeline(
                "Outline Pipeline",
                device,
                Some(&render_pipeline_layout),
                final_format,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::BuildMipmaps => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Hdr)), //0
            ];

            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("BuildMipmaps Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });

            let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/mips.wgsl"));

            let buffer_desc = &[];

            let pipeline_desc = PipelineDesc {
                depth_stencil: None,
                ..Default::default()
            };

            pipeline_desc.build_pipeline(
                "BuildMipmaps Pipeline",
                device,
                Some(&render_pipeline_layout),
                wgpu::TextureFormat::Rgba8Unorm,
                shader,
                buffer_desc,
            )
        }
        PipelineKind::Convert => {
            let layouts = [
                Some(gpu_resource_manager.get_bindgroup_layout(BindgroupLayoutKind::Depth)), //0
                None,                                                                        //1
                None,                                                                        //2
                None,                                                                        //3
            ];
            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("ShadowMap Pipeline Layout"),
                    bind_group_layouts: &layouts,
                    immediate_size: 0,
                });

            let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/convert.wgsl"));
            let buffer_desc = &[];
            let pipeline_desc = PipelineDesc {
                depth_stencil: None,
                ..Default::default()
            };

            pipeline_desc.build_pipeline(
                "Convert Pipeline",
                device,
                Some(&render_pipeline_layout),
                wgpu::TextureFormat::Rgba8Unorm,
                shader,
                buffer_desc,
            )
        }
    }
}

fn create_cs_pipeline(
    device: &wgpu::Device,
    #[allow(unused)] gpu_resource_manager: &GpuManager,
    kind: CsPipelineKind,
) -> wgpu::ComputePipeline {
    match kind {
        CsPipelineKind::BuildMipmaps => {
            let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/cs_mips.wgsl"));

            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("BuildMipmaps CS Pipeline"),
                layout: None,
                module: &shader,
                entry_point: Some("cs_main"),
                compilation_options: Default::default(),
                cache: None,
            })
        }
        CsPipelineKind::CopyToMip0 => {
            let shader =
                device.create_shader_module(wgpu::include_wgsl!("shaders/cs_hdr_to_mip0.wgsl"));

            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("BuildMipmaps CS Pipeline"),
                layout: None,
                module: &shader,
                entry_point: Some("cs_main"),
                compilation_options: Default::default(),
                cache: None,
            })
        }
    }
}
