// step for creating IBL environment
// CreateBRDFintegrationMap();
// CubemapFromHDR(skybox);
// CreateIrradiance(skybox);
// CreatePrefilterMap(skybox);

use std::collections::HashMap;

use crate::assets::IblId;

use super::ibl_impl::*;
use super::*;

trait Size {
    fn estimated_size(&self) -> usize;
}

impl Size for wgpu::Texture {
    fn estimated_size(self: &wgpu::Texture) -> usize {
        let extent = self.size();
        let format = self.format();

        (extent.width
            * extent.height
            * extent.depth_or_array_layers
            * format.target_pixel_byte_cost().unwrap_or(4)) as usize
    }
}

pub struct GpuIbl {
    cube_map: wgpu::Texture,
    irradiance_map: wgpu::Texture,
    prefilter_map: wgpu::Texture,
    sampler: wgpu::Sampler,
    cube_map_view: wgpu::TextureView,
    irradiance_view: wgpu::TextureView,
    prefilter_view: wgpu::TextureView,
    brdf_lut_view: wgpu::TextureView,
}

impl GpuIbl {
    pub fn get_sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }
    pub fn get_irradiance_view(&self) -> &wgpu::TextureView {
        &self.irradiance_view
    }
    pub fn get_prefilter_view(&self) -> &wgpu::TextureView {
        &self.prefilter_view
    }
    pub fn get_brdf_lut_view(&self) -> &wgpu::TextureView {
        &self.brdf_lut_view
    }
    pub fn get_cubemap_view(&self) -> &wgpu::TextureView {
        &self.cube_map_view
    }

    fn estimated_size(&self) -> usize {
        self.cube_map.estimated_size()
            + self.irradiance_map.estimated_size()
            + self.prefilter_map.estimated_size()
    }
}

impl HasGpuStats for IblManager {
    fn get_stats(&self) -> GpuResourceStats {
        self.stats.clone()
    }
}

pub struct IblManager {
    _brdf_lut: wgpu::Texture,
    brdf_lut_view: wgpu::TextureView,
    map: HashMap<IblId, GpuIbl>,
    stats: GpuResourceStats,
}

impl IblManager {
    pub fn new(gpu: &GpuContextRef) -> Self {
        let brdf_lut = BRDFLUTBuilder::build(gpu.device, gpu.queue);
        let brdf_lut_view = brdf_lut.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _brdf_lut: brdf_lut,
            brdf_lut_view,
            map: HashMap::new(),
            stats: GpuResourceStats::default(),
        }
    }

    pub fn insert(&mut self, id: IblId, gpu_ibl: GpuIbl) {
        if let Some(previous) = self.map.insert(id, gpu_ibl) {
            self.stats.remove(previous.estimated_size());
        }
        self.stats.add(self.map[&id].estimated_size());
    }

    pub fn get(&self, id: &IblId) -> Option<&GpuIbl> {
        self.map.get(id)
    }

    #[allow(unused)]
    pub fn remove(&mut self, id: IblId) {
        if let Some(gpu_ibl) = self.map.remove(&id) {
            self.stats.remove(gpu_ibl.estimated_size());
        }
    }

    pub fn create(&self, hdr: &GpuTexture, gpu: &GpuContextRef) -> GpuIbl {
        let cube_map = EquirectangularToCubemap::build(hdr, gpu.device, gpu.queue, 512);
        let _irradiance_map = IrrarianceMap::build(&cube_map, gpu.device, gpu.queue);
        let _prefilter_map = PrefilterMap::build(gpu.device, gpu.queue, &cube_map);
        let cube_map_view = cube_map.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let irradiance_view = _irradiance_map.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let prefilter_view = _prefilter_map.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        GpuIbl {
            cube_map,
            cube_map_view,
            irradiance_map: _irradiance_map,
            irradiance_view,
            prefilter_map: _prefilter_map,
            prefilter_view,
            sampler,
            brdf_lut_view: self.brdf_lut_view.clone(),
        }
    }
}
