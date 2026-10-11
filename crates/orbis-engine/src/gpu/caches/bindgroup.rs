use crate::{
    assets::texture_asset::{ColorSpace, SamplerDesc},
    gpu::{Dimension::Array, GpuContextRef},
};

use super::*;

use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Debug, Clone, Copy, EnumIter)]
pub enum BindgroupKind {
    Camera,
    Perframe,
    LightIcon,
    PbrMap,
    Skybox,
    SkyboxBlur,
}

pub struct BindgroupCache {
    bg: Vec<wgpu::BindGroup>,
}

impl BindgroupCache {
    pub fn new(
        gpu: &GpuContextRef,
        buffer_cache: &BufferCache,
        framebuffer_cache: &FramebufferCache,
        layouts: &BindgroupLayoutCache,
    ) -> Self {
        let bg: Vec<wgpu::BindGroup> = BindgroupKind::iter()
            .map(|kind| Self::create(gpu, buffer_cache, &framebuffer_cache, layouts, kind))
            .collect();
        Self { bg }
    }
    pub fn get(&self, kind: BindgroupKind) -> &wgpu::BindGroup {
        &self.bg[kind as usize]
    }
    pub fn get_mut(&mut self, kind: BindgroupKind) -> &mut wgpu::BindGroup {
        &mut self.bg[kind as usize]
    }

    pub(crate) fn reset_ibl(
        &mut self,
        gpu: &GpuContextRef,
        buffer_cache: &BufferCache,
        framebuffer_cache: &FramebufferCache,
        layouts: &BindgroupLayoutCache,
    ) {
        for kind in [
            BindgroupKind::PbrMap,
            BindgroupKind::Skybox,
            BindgroupKind::SkyboxBlur,
        ] {
            self.bg[kind as usize] =
                Self::create(gpu, buffer_cache, framebuffer_cache, layouts, kind);
        }
    }
}

impl BindgroupCache {
    fn create(
        gpu: &GpuContextRef,
        buffer_cache: &BufferCache,
        framebuffer_cache: &FramebufferCache,
        layouts: &BindgroupLayoutCache,
        kind: BindgroupKind,
    ) -> wgpu::BindGroup {
        match kind {
            BindgroupKind::Camera => gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: layouts.get(BindgroupLayoutKind::Camera),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer_cache.get(BufferKind::Camera).as_entire_binding(),
                }],
                label: Some("Camera Bind Group"),
            }),
            BindgroupKind::Perframe => {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: layouts.get(BindgroupLayoutKind::PerFrame),
                    entries: &[
                        // Camera
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer_cache.get(BufferKind::Camera).as_entire_binding(),
                        },
                        // GLobals
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: buffer_cache.get(BufferKind::Globals).as_entire_binding(),
                        },
                        // Lights
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: buffer_cache.get(BufferKind::Lights).as_entire_binding(),
                        },
                    ],
                    label: Some("PerFrame Bind Group"),
                })
            }
            BindgroupKind::LightIcon => {
                let texture =
                    GpuTextureBuilder::from_static(&static_textures::LIGHTBULB_STATIC_TEXTURE)
                        .sampler(SamplerDesc::LinearRepeat)
                        .build(gpu);

                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: layouts.get(BindgroupLayoutKind::LightIcon),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&texture.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&texture.view),
                        },
                    ],
                    label: Some("light icon_bind_group"),
                })
            }
            BindgroupKind::PbrMap => {
                let scene_view = framebuffer_cache.get_view(FramebufferKind::OpaqueWithMips);
                let scene_sampler = framebuffer_cache.get_sampler(FramebufferKind::OpaqueWithMips);

                let dummy_texture =
                    GpuTextureBuilder::from_static(&static_textures::WHITE_STATIC_TEXTURE)
                        .build(gpu);

                let dummy_cube =
                    GpuTextureBuilder::from_static(&static_textures::WHITE_STATIC_TEXTURE)
                        .dimension(Dimension::Cube)
                        .format(ColorSpace::Rgba8)
                        .usage(GpuTextureUsage::SampledTexture)
                        .sampler(SamplerDesc::LinearRepeat)
                        .label("dummy Cube white texture")
                        .build(gpu);

                //Dummy Shadowmaps
                let dummy_depth_texture = GpuTextureBuilder::from_empty(1, 1)
                    .format(ColorSpace::Depth32f)
                    .dimension(Array(1))
                    .usage(GpuTextureUsage::SampledTexture)
                    .sampler(SamplerDesc::DepthComparison)
                    .label("dummy shadow_texture depth")
                    .build(gpu);

                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: layouts.get(BindgroupLayoutKind::PbrMaps),
                    entries: &[
                        // sampler
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&dummy_cube.sampler),
                        },
                        // irradiance texture
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&dummy_cube.view),
                        },
                        // prefiltered texture
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&dummy_cube.view),
                        },
                        // brdf_lut texture
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&dummy_texture.view),
                        },
                        // opaque scene sampler
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&scene_sampler),
                        },
                        // opaque scene texture
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::TextureView(&scene_view),
                        },
                        // shadowmap sampler
                        wgpu::BindGroupEntry {
                            binding: 6,
                            resource: wgpu::BindingResource::Sampler(&dummy_depth_texture.sampler),
                        },
                        // shadowmap texture
                        wgpu::BindGroupEntry {
                            binding: 7,
                            resource: wgpu::BindingResource::TextureView(&dummy_depth_texture.view),
                        },
                    ],
                    label: Some("Dummy PbrMap BindGroup"),
                })
            }
            BindgroupKind::Skybox => {
                let cube = GpuTextureBuilder::from_static(&static_textures::WHITE_STATIC_TEXTURE)
                    .dimension(Dimension::Cube)
                    .format(ColorSpace::Rgba8)
                    .usage(GpuTextureUsage::SampledTexture)
                    .sampler(SamplerDesc::LinearRepeat)
                    .label("Cube white texture")
                    .build(gpu);

                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: layouts.get(BindgroupLayoutKind::Skybox),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&cube.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&cube.view),
                        },
                    ],
                    label: Some("Dummy Skybox BindGroup"),
                })
            }
            BindgroupKind::SkyboxBlur => {
                let cube = GpuTextureBuilder::from_static(&static_textures::WHITE_STATIC_TEXTURE)
                    .dimension(Dimension::Cube)
                    .format(ColorSpace::Rgba8)
                    .usage(GpuTextureUsage::SampledTexture)
                    .sampler(SamplerDesc::LinearRepeat)
                    .label("Cube white texture")
                    .build(gpu);

                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: layouts.get(BindgroupLayoutKind::Skybox),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&cube.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&cube.view),
                        },
                    ],
                    label: Some("Dummy SkyboxBlur BindGroup"),
                })
            }
        }
    }
}
