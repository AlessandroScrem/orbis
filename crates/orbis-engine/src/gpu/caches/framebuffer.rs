use std::{collections::HashMap, sync::Arc};

use crate::{
    assets::texture_asset::{ColorSpace, SamplerDesc},
    gpu::GpuContextRef,
    renderer::transient_pool::{
        TransientRequest, TransientResourcePool, TransientResourceResolver, TransientTextureDescriptor,
        TransientTextureFormat, TransientTextureUsage,
    },
};

use super::*;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Debug, Clone, Copy, EnumIter, PartialEq, Eq, Hash)]
pub enum FramebufferKind {
    Hdr,
    OpaqueWithMips,
    EntityId,
    Depth,
}

pub struct Framebuffer {
    texture: Arc<GpuTexture>,
    bind_group: wgpu::BindGroup,
}

pub struct FramebufferCache {
    framebuffers: Vec<Framebuffer>,
    transient_pool: TransientResourcePool<TransientTextureDescriptor, Arc<GpuTexture>>,
    width: u32,
    height: u32,
}

impl FramebufferCache {
    pub fn new(
        gpu: &GpuContextRef,
        layouts: &BindgroupLayoutCache,
        width: u32,
        height: u32,
    ) -> Self {
        let mut cache = Self {
            framebuffers: Vec::new(),
            transient_pool: TransientResourcePool::new(),
            width,
            height,
        };
        cache.recreate_framebuffers(gpu, layouts);
        cache
    }

    pub fn resize(
        &mut self,
        gpu: &GpuContextRef,
        layouts: &BindgroupLayoutCache,
        width: u32,
        height: u32,
    ) {
        if width == 0 || height == 0 || (self.width == width && self.height == height) {
            return;
        }

        self.width = width;
        self.height = height;
        self.transient_pool.clear();
        self.recreate_framebuffers(gpu, layouts);
    }

    /// Allocates the frame's logical framebuffer resources according to RenderGraph lifetimes.
    ///
    /// The pool owns the actual GPU textures. Compatible resources may point to the same
    /// allocation only when their inclusive lifetimes do not overlap. Bind groups are
    /// recreated for the current assignments so they always reference the resolved texture.
    pub(crate) fn allocate_transient_frame(
        &mut self,
        gpu: &GpuContextRef,
        layouts: &BindgroupLayoutCache,
        lifetimes: &[(FramebufferKind, usize, usize)],
    ) {
        // Adopt the initialization/resize allocations on the first frame instead
        // of creating a second set of textures. This also keeps bind groups that
        // already reference these textures valid.
        if self.transient_pool.is_empty() {
            for kind in FramebufferKind::iter() {
                let descriptor = self.descriptor(kind);
                let texture = Arc::clone(&self.framebuffers[kind as usize].texture);
                self.transient_pool.insert(descriptor, texture);
            }
        }

        let requests: Vec<_> = FramebufferKind::iter()
            .map(|kind| {
                let (first_use, last_use) = lifetimes
                    .iter()
                    .find(|(candidate, _, _)| *candidate == kind)
                    .map(|(_, first, last)| (*first, *last))
                    // An untracked resource must remain exclusive for the whole graph.
                    .unwrap_or((0, usize::MAX));
                TransientRequest::new(self.descriptor(kind), first_use, last_use)
            })
            .collect();

        let width = self.width;
        let height = self.height;
        let slots = self.transient_pool.allocate_frame(&requests, |descriptor| {
            Arc::new(Self::create_texture(gpu, *descriptor, width, height))
        });

        let resolver = TransientResourceResolver::from_slots(
            FramebufferKind::iter().zip(slots).collect(),
        );
        let framebuffers = FramebufferKind::iter()
            .map(|kind| {
                let texture = Arc::clone(
                    resolver
                        .resolve(&kind, &self.transient_pool)
                        .expect("transient framebuffer resource must resolve"),
                );
                let bind_group = Self::create_bind_group(gpu, layouts, kind, &texture);
                Framebuffer { texture, bind_group }
            })
            .collect();

        self.framebuffers = framebuffers;
    }

    pub fn get_texture(&self, kind: FramebufferKind) -> &wgpu::Texture {
        &self.framebuffers[kind as usize].texture.inner
    }

    pub fn get_sampler(&self, kind: FramebufferKind) -> &wgpu::Sampler {
        &self.framebuffers[kind as usize].texture.sampler
    }

    pub fn get_view(&self, kind: FramebufferKind) -> &wgpu::TextureView {
        &self.framebuffers[kind as usize].texture.view
    }

    pub fn get_view_mips(&self, kind: FramebufferKind) -> &wgpu::TextureView {
        &self.framebuffers[kind as usize].texture.view_mips
    }

    pub fn get_bg(&self, kind: FramebufferKind) -> &wgpu::BindGroup {
        &self.framebuffers[kind as usize].bind_group
    }

    #[allow(unused)]
    pub fn get_map(&self) -> HashMap<FramebufferKind, &GpuTexture> {
        [FramebufferKind::Hdr]
            .into_iter()
            .map(|kind| (kind, self.framebuffers[kind as usize].texture.as_ref()))
            .collect()
    }

    fn recreate_framebuffers(&mut self, gpu: &GpuContextRef, layouts: &BindgroupLayoutCache) {
        self.framebuffers = FramebufferKind::iter()
            .map(|kind| {
                let descriptor = self.descriptor(kind);
                let texture = Arc::new(Self::create_texture(
                    gpu,
                    descriptor,
                    self.width,
                    self.height,
                ));
                let bind_group = Self::create_bind_group(gpu, layouts, kind, &texture);
                Framebuffer { texture, bind_group }
            })
            .collect();
    }

    fn descriptor(&self, kind: FramebufferKind) -> TransientTextureDescriptor {
        let (format, mip_level_count, usage) = match kind {
            FramebufferKind::Hdr => (
                TransientTextureFormat::Rgba16Float,
                1,
                TransientTextureUsage::RenderTarget,
            ),
            FramebufferKind::OpaqueWithMips => (
                TransientTextureFormat::Rgba8Unorm,
                8,
                TransientTextureUsage::SampledTextureStorage,
            ),
            FramebufferKind::EntityId => (
                TransientTextureFormat::Rg32Uint,
                1,
                TransientTextureUsage::EntityId,
            ),
            FramebufferKind::Depth => (
                TransientTextureFormat::Depth32Float,
                1,
                TransientTextureUsage::DepthTarget,
            ),
        };

        TransientTextureDescriptor {
            width: self.width,
            height: self.height,
            format,
            mip_level_count,
            usage,
        }
    }

    fn create_texture(
        gpu: &GpuContextRef,
        descriptor: TransientTextureDescriptor,
        width: u32,
        height: u32,
    ) -> GpuTexture {
        let format = match descriptor.format {
            TransientTextureFormat::Rgba8Unorm => ColorSpace::Rgba8,
            TransientTextureFormat::Rgba8UnormSrgb => ColorSpace::Srgba8,
            TransientTextureFormat::Rgba16Float => ColorSpace::Rgbaf16,
            TransientTextureFormat::Rgba32Float => ColorSpace::Rgbaf32,
            TransientTextureFormat::Rg32Uint => ColorSpace::Rg32ui,
            TransientTextureFormat::Depth32Float => ColorSpace::Depth32f,
        };
        let usage = match descriptor.usage {
            TransientTextureUsage::RenderTarget => GpuTextureUsage::RenderTarget,
            TransientTextureUsage::EntityId => GpuTextureUsage::EntityId,
            TransientTextureUsage::DepthTarget => GpuTextureUsage::DepthTarget,
            TransientTextureUsage::SampledTexture => GpuTextureUsage::SampledTexture,
            TransientTextureUsage::SampledTextureStorage => GpuTextureUsage::SampledTextureStorage,
        };
        let sampler = match descriptor.usage {
            TransientTextureUsage::SampledTextureStorage => SamplerDesc::LinearClampMipmap,
            _ => SamplerDesc::NearestClamp,
        };

        let mut builder = GpuTextureBuilder::from_empty(width, height)
            .format(format)
            .usage(usage)
            .sampler(sampler);
        if descriptor.mip_level_count > 1 {
            builder = builder.with_mips(descriptor.mip_level_count);
        }
        builder.label("Transient framebuffer texture").build(gpu)
    }

    fn create_bind_group(
        gpu: &GpuContextRef,
        layouts: &BindgroupLayoutCache,
        kind: FramebufferKind,
        texture: &GpuTexture,
    ) -> wgpu::BindGroup {
        let layout_kind = match kind {
            FramebufferKind::Hdr | FramebufferKind::OpaqueWithMips => BindgroupLayoutKind::Hdr,
            FramebufferKind::EntityId => BindgroupLayoutKind::EntityId,
            FramebufferKind::Depth => BindgroupLayoutKind::Depth,
        };
        let label = match kind {
            FramebufferKind::Hdr => "Hdr_bind_group",
            FramebufferKind::OpaqueWithMips => "Hdr_Opaque_bind_group",
            FramebufferKind::EntityId => "entity_id_bind_group",
            FramebufferKind::Depth => "depth_bind_group",
        };
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: layouts.get(layout_kind),
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
        })
    }
}
