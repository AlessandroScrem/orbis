use std::collections::HashMap;

use super::*;
use crate::gpu::ibl::GpuIbl;

pub struct GpuManager {
    layout_cache: BindgroupLayoutCache,
    framebuffer_cache: FramebufferCache,
    buffer_cache: BufferCache,
    bindgroup_cache: BindgroupCache,
}

impl GpuManager {
    pub fn new(gpu: &GpuContextRef, width: u32, height: u32) -> Self {
        let layout_cache = BindgroupLayoutCache::new(gpu.device);
        let buffer_cache = BufferCache::new(gpu.device);
        let framebuffer_cache = FramebufferCache::new(gpu, &layout_cache, width, height);
        let bindgroup_cache =
            BindgroupCache::new(gpu, &buffer_cache, &framebuffer_cache, &layout_cache);

        Self {
            layout_cache,
            framebuffer_cache,
            buffer_cache,
            bindgroup_cache,
        }
    }

    pub fn get_bindgroup_layout(&self, kind: BindgroupLayoutKind) -> &wgpu::BindGroupLayout {
        self.layout_cache.get(kind)
    }

    pub fn get_framebuffer_view(&self, kind: FramebufferKind) -> &wgpu::TextureView {
        self.framebuffer_cache.get_view(kind)
    }

    pub fn get_framebuffer_sampler(&self, kind: FramebufferKind) -> &wgpu::Sampler {
        self.framebuffer_cache.get_sampler(kind)
    }

    pub fn get_framebuffer_texture(&self, kind: FramebufferKind) -> &wgpu::Texture {
        self.framebuffer_cache.get_texture(kind)
    }
    #[allow(unused)]
    pub fn get_framebuffers(&self) -> HashMap<FramebufferKind, &GpuTexture> {
        self.framebuffer_cache.get_map()
    }
    pub fn get_framebuffer_bg(&self, kind: FramebufferKind) -> &wgpu::BindGroup {
        self.framebuffer_cache.get_bg(kind)
    }

    pub fn resize_frame(&mut self, gpu: &GpuContextRef, width: u32, height: u32) {
        self.framebuffer_cache
            .resize(gpu, &self.layout_cache, width, height);
    }

    pub fn get_bindgroup(&self, kind: BindgroupKind) -> &wgpu::BindGroup {
        self.bindgroup_cache.get(kind)
    }
    pub fn get_buffer(&self, kind: BufferKind) -> &wgpu::Buffer {
        self.buffer_cache.get(kind)
    }

    pub fn update_buffer<T: bytemuck::Pod>(
        &self,
        queue: &wgpu::Queue,
        kind: BufferKind,
        data: &[T],
    ) {
        self.buffer_cache.write(queue, kind, data);
    }

    pub fn get_ibl_skybox_bg(&self, blur_flag: bool) -> &wgpu::BindGroup {
        if blur_flag {
            &self.bindgroup_cache.get(BindgroupKind::SkyboxBlur)
        } else {
            &self.bindgroup_cache.get(BindgroupKind::Skybox)
        }
    }

    pub fn clear_ibl_bindgroups(&mut self, gpu: &GpuContextRef) {
        self.bindgroup_cache.reset_ibl(
            gpu,
            &self.buffer_cache,
            &self.framebuffer_cache,
            &self.layout_cache,
        );
    }

    pub fn replace_pbrmap_skybox_bindgroup(
        &mut self,
        ibl: Option<&GpuIbl>,
        shadow_mgr: &ShadowManager,
        device: &wgpu::Device,
    ) {
        if let Some(ibl) = ibl {
            info!("updating pbrmap bindgroup");

            let bg = create_pbrmap_bindgroup(
                device,
                ibl,
                shadow_mgr,
                &self.layout_cache,
                &self.framebuffer_cache,
            );
            self.update_bindgroup(BindgroupKind::PbrMap, bg);

            let bg = create_skybox_bindgroup(device, ibl, &self.layout_cache);
            self.update_bindgroup(BindgroupKind::Skybox, bg);

            let bg = create_skybox_blur_bindgroup(device, ibl, &self.layout_cache);
            self.update_bindgroup(BindgroupKind::SkyboxBlur, bg);
        }
    }

    //mutables
    fn update_bindgroup(&mut self, kind: BindgroupKind, bg: wgpu::BindGroup) {
        *self.bindgroup_cache.get_mut(kind) = bg;
    }
}

fn create_pbrmap_bindgroup(
    device: &wgpu::Device,
    ibl: &GpuIbl,
    shadow_mgr: &ShadowManager,
    layout_cache: &BindgroupLayoutCache,
    framebuffer_cache: &FramebufferCache,
) -> wgpu::BindGroup {
    let shadow_map_sampler = shadow_mgr.get_sampler();
    let shadow_map_views = shadow_mgr.get_views();
    let sampler = ibl.get_sampler();
    let brdf_lut_view = ibl.get_brdf_lut_view();
    let irradiance_view = ibl.get_irradiance_view();
    let prefilter_view = ibl.get_prefilter_view();

    let opaque_sampler = framebuffer_cache.get_sampler(FramebufferKind::OpaqueWithMips);
    let opaque_view = framebuffer_cache.get_view_mips(FramebufferKind::OpaqueWithMips);

    let entries = vec![
        // sampler
        wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Sampler(sampler),
        },
        // irradiance texture
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(irradiance_view),
        },
        // prefiltered texture
        wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::TextureView(prefilter_view),
        },
        // brdf_lut texture
        wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(brdf_lut_view),
        },
        // opaque scene sampler
        wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::Sampler(opaque_sampler),
        },
        // opaque scene texture
        wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::TextureView(opaque_view),
        },
        // shadowmap sampler
        wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::Sampler(shadow_map_sampler),
        },
        // shadowmap texture
        wgpu::BindGroupEntry {
            binding: 7,
            resource: wgpu::BindingResource::TextureView(&shadow_map_views),
        },
    ];

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("PbrMap Bindgroup"),
        layout: layout_cache.get(BindgroupLayoutKind::PbrMaps),
        entries: &entries,
    })
}

fn create_skybox_bindgroup(
    device: &wgpu::Device,
    ibl: &GpuIbl,
    layout_cache: &BindgroupLayoutCache,
) -> wgpu::BindGroup {
    let label = Some("Skybox bind_group");

    let sampler = ibl.get_sampler();
    let cubemap_view = ibl.get_cubemap_view();

    let entries = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Sampler(sampler),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(cubemap_view),
        },
    ];

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label,
        layout: layout_cache.get(BindgroupLayoutKind::Skybox),
        entries: &entries,
    })
}
fn create_skybox_blur_bindgroup(
    device: &wgpu::Device,
    ibl: &GpuIbl,
    layout_cache: &BindgroupLayoutCache,
) -> wgpu::BindGroup {
    let label = Some("Skybox blur bind_group");

    let sampler = ibl.get_sampler();
    let irradiance_view = ibl.get_irradiance_view();

    let entries = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Sampler(sampler),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(irradiance_view),
        },
    ];

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label,
        layout: layout_cache.get(BindgroupLayoutKind::Skybox),
        entries: &entries,
    })
}

#[cfg(test)]
mod resize_tests {
    use super::*;
    use crate::test_utils;

    #[test]
    fn zero_sized_resize_keeps_valid_framebuffers() {
        let gpu = test_utils::get_gpu_context_test();
        let mut manager = GpuManager::new(&gpu, 4, 4);

        manager.resize_frame(&gpu, 0, 0);

        assert_eq!(
            manager
                .get_framebuffer_texture(FramebufferKind::Hdr)
                .width(),
            4
        );
        assert_eq!(
            manager
                .get_framebuffer_texture(FramebufferKind::Hdr)
                .height(),
            4
        );
    }
}
