use crate::ResourceId;
use std::collections::HashMap;

use super::*;

use crate::{
    assets::material_desc::MaterialDesc,
    gpu::{GpuResourceStats, HasGpuStats},
    renderer::uniform::MaterialUniform,
};
use wgpu::util::DeviceExt;

#[derive(Default)]
pub struct GpuMaterial {
    pub bind_group: Option<wgpu::BindGroup>,
    revision: u64,
}

impl GpuMaterial {
    const MATERIAL_SIZE: usize = size_of::<MaterialUniform>();
    fn estimated_size() -> usize {
        Self::MATERIAL_SIZE
    }

    pub fn new(
        texture_cache: &GpuTextureCache,
        material_desc: &MaterialDesc,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> GpuMaterial {
        let uniform_buffer = create_material_uniform_from_desc(device, material_desc);

        let bindgroup = create_material_bindgroup_from_desc(
            device,
            texture_cache,
            material_desc,
            &uniform_buffer,
            layout,
        );

        GpuMaterial {
            bind_group: Some(bindgroup),
            revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Default)]
pub struct GpuMaterialCache {
    map: HashMap<ResourceId, GpuMaterial>,
    stats: GpuResourceStats,
}

impl HasGpuStats for GpuMaterialCache {
    fn get_stats(&self) -> GpuResourceStats {
        self.stats.clone()
    }
}

impl GpuMaterialCache {
    pub fn insert(&mut self, id: ResourceId, mut gpu_material: GpuMaterial) {
        if let Some(previous) = self.map.get(&id) {
            gpu_material.revision = previous.revision.wrapping_add(1);
        } else {
            self.stats.add(GpuMaterial::estimated_size());
        }
        self.map.insert(id, gpu_material);
    }

    pub fn get(&self, id: &ResourceId) -> Option<&GpuMaterial> {
        self.map.get(id)
    }

    pub fn remove(&mut self, id: ResourceId) {
        if self.map.remove(&id).is_some() {
            self.stats.remove(GpuMaterial::estimated_size());
        }
    }
}

pub fn create_material_bindgroup_from_desc(
    device: &wgpu::Device,
    texture_cache: &GpuTextureCache,
    material_desc: &MaterialDesc,
    uniform_buffer: &wgpu::Buffer,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    // Default sampler for all material textures (can be overridden by texture asset)
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    use crate::assets::material_desc::MaterialTextureSlot::*;

    pub struct GpuTextures<'a>(
        pub [&'a GpuTexture; crate::assets::material_desc::MATERIAL_TEXTURE_COUNT],
    );

    use crate::assets::material_desc::MaterialTextureSlot;
    impl<'a> std::ops::Index<MaterialTextureSlot> for GpuTextures<'a> {
        type Output = GpuTexture;

        fn index(&self, slot: MaterialTextureSlot) -> &Self::Output {
            self.0[slot as usize]
        }
    }

    fn resolve_textures<'a>(
        texture_cache: &'a GpuTextureCache,
        desc: &MaterialDesc,
    ) -> GpuTextures<'a> {
        use crate::assets::material_desc::MaterialTextureSlot::*;

        GpuTextures([
            texture_cache.get_or(desc.texture(BaseColor), CacheTextureSlot::White),
            texture_cache.get_or(desc.texture(Normal), CacheTextureSlot::White),
            texture_cache.get_or(desc.texture(MetallicRoughness), CacheTextureSlot::White),
            texture_cache.get_or(desc.texture(Emissive), CacheTextureSlot::White),
            texture_cache.get_or(desc.texture(Occlusion), CacheTextureSlot::White),
            texture_cache.get_or(desc.texture(Transmission), CacheTextureSlot::White),
            texture_cache.get_or(desc.texture(Volume), CacheTextureSlot::White),
        ])
    }

    let textures = resolve_textures(texture_cache, material_desc);

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: bind_group_layout,
        label: Some("Material  bind_group"),
        entries: &[
            // uniform buffer
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            // sampler
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            // main texture
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&textures[BaseColor].view),
            },
            // normal texture
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&textures[Normal].view),
            },
            // metallic_roughness texture
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&textures[MetallicRoughness].view),
            },
            // material emissive
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&textures[Emissive].view),
            },
            // material occlusion
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&textures[Occlusion].view),
            },
            // material transmission
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(&textures[Transmission].view),
            },
            // material volume
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(&textures[Volume].view),
            },
        ],
    })
}

pub fn create_material_uniform_from_desc(
    device: &wgpu::Device,
    material_desc: &MaterialDesc,
) -> wgpu::Buffer {
    let uniform = MaterialUniform::from(material_desc);

    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Material Uniform Buffer"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

#[cfg(test)]
mod tests {
    use super::{GpuMaterial, GpuMaterialCache};
    use crate::ResourceId;

    #[test]
    fn replacing_material_increments_revision() {
        let mut cache = GpuMaterialCache::default();
        let id = ResourceId::new();

        cache.insert(id, GpuMaterial::default());
        assert_eq!(cache.get(&id).unwrap().revision(), 0);

        cache.insert(id, GpuMaterial::default());
        assert_eq!(cache.get(&id).unwrap().revision(), 1);

        cache.insert(id, GpuMaterial::default());
        assert_eq!(cache.get(&id).unwrap().revision(), 2);
    }

    #[test]
    fn inserting_another_material_starts_its_revision_at_zero() {
        let mut cache = GpuMaterialCache::default();
        let first = ResourceId::new();
        let second = ResourceId::new();

        cache.insert(first, GpuMaterial::default());
        cache.insert(first, GpuMaterial::default());
        cache.insert(second, GpuMaterial::default());

        assert_eq!(cache.get(&first).unwrap().revision(), 1);
        assert_eq!(cache.get(&second).unwrap().revision(), 0);
    }
}
