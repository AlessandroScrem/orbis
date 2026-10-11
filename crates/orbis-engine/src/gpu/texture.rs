use super::static_textures::StaticTexture;
use crate::assets::texture_asset::{ColorSpace, SamplerDesc};
use crate::assets::texture_upload::TextureData;

use crate::gpu::GpuContextRef;

use crate::prelude::*;
use std::sync::Arc;

pub enum TextureSource<'a> {
    Cpu(TextureData),
    Static(&'a StaticTexture),
}

pub enum Dimension {
    D2,
    Cube,
    Array(u32),
}

pub struct GpuTexture {
    pub inner: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub view_mips: Arc<wgpu::TextureView>,
    pub extent: wgpu::Extent3d,
    pub sampler: wgpu::Sampler,
    pub estimated_size: usize,
    #[allow(unused)]
    pub format: ColorSpace,
}

pub struct GpuTextureBuilder<'a> {
    width: u32,
    height: u32,
    with_mips: Option<u32>,
    source: Option<TextureSource<'a>>,
    sampler: Option<SamplerDesc>,
    format: ColorSpace,
    dimension: Dimension,
    usage: GpuTextureUsage,
    label: Option<&'a str>,
}

pub enum GpuTextureUsage {
    EntityId,
    RenderTarget,
    DepthTarget,
    SampledTexture,
    SampledTextureStorage,
}
impl From<GpuTextureUsage> for wgpu::TextureUsages {
    fn from(tu: GpuTextureUsage) -> Self {
        match tu {
            GpuTextureUsage::EntityId => {
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
            }
            GpuTextureUsage::RenderTarget => {
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
            }
            GpuTextureUsage::DepthTarget => {
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT
            }
            GpuTextureUsage::SampledTexture => {
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
            }
            GpuTextureUsage::SampledTextureStorage => {
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::STORAGE_BINDING
            }
        }
    }
}
impl From<ColorSpace> for wgpu::TextureFormat {
    fn from(cs: ColorSpace) -> Self {
        match cs {
            ColorSpace::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            ColorSpace::Srgba8 => wgpu::TextureFormat::Rgba8UnormSrgb,
            ColorSpace::Rgbaf16 => wgpu::TextureFormat::Rgba16Float,
            ColorSpace::Rgbaf32 => wgpu::TextureFormat::Rgba32Float,
            ColorSpace::Rg32ui => wgpu::TextureFormat::Rg32Uint,
            ColorSpace::Depth32f => wgpu::TextureFormat::Depth32Float,
        }
    }
}

impl From<SamplerDesc> for wgpu::SamplerDescriptor<'_> {
    fn from(sd: SamplerDesc) -> Self {
        match sd {
            SamplerDesc::LinearRepeat => wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,

                ..Default::default()
            },
            SamplerDesc::LinearClampMipmap => wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            },
            SamplerDesc::NearestClamp => wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            },
            SamplerDesc::DepthComparison => wgpu::SamplerDescriptor {
                compare: Some(wgpu::CompareFunction::LessEqual),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            },
        }
    }
}

impl<'a> GpuTextureBuilder<'a> {
    pub fn from_cpu(data: TextureData) -> GpuTextureBuilder<'static> {
        GpuTextureBuilder {
            label: None,
            format: data.format,
            width: data.width,
            height: data.height,
            dimension: Dimension::D2,
            usage: GpuTextureUsage::SampledTexture,
            sampler: Some(SamplerDesc::LinearRepeat),
            source: Some(TextureSource::Cpu(data)),
            with_mips: None,
        }
    }

    pub fn from_static(data: &'static StaticTexture) -> GpuTextureBuilder<'static> {
        GpuTextureBuilder {
            label: None,
            format: data.format,
            width: data.width,
            height: data.height,
            dimension: Dimension::D2,
            usage: GpuTextureUsage::SampledTexture,
            sampler: Some(SamplerDesc::LinearRepeat),
            source: Some(TextureSource::Static(data)),
            with_mips: None,
        }
    }

    pub fn from_empty(width: u32, height: u32) -> GpuTextureBuilder<'static> {
        GpuTextureBuilder {
            label: None,
            width,
            height,
            dimension: Dimension::D2,
            format: ColorSpace::Rgba8,
            usage: GpuTextureUsage::RenderTarget,
            sampler: None,
            source: None,
            with_mips: None,
        }
    }

    pub fn format(mut self, format: ColorSpace) -> Self {
        self.format = format;
        self
    }

    pub fn dimension(mut self, dimension: Dimension) -> Self {
        self.dimension = dimension;
        self
    }

    pub fn usage(mut self, usage: GpuTextureUsage) -> Self {
        self.usage = usage;
        self
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn sampler(mut self, sampler: SamplerDesc) -> Self {
        self.sampler = Some(sampler);
        self
    }

    pub fn with_mips(mut self, max_mips: u32) -> Self {
        self.with_mips = Some(max_mips);
        self
    }
}

impl<'a> GpuTextureBuilder<'a> {
    pub fn build(self, gpu: &GpuContextRef) -> GpuTexture {
        let (layers, view_dimension) = match self.dimension {
            Dimension::D2 => (1, wgpu::TextureViewDimension::D2),
            Dimension::Cube => (6, wgpu::TextureViewDimension::Cube),
            Dimension::Array(layers) => (layers, wgpu::TextureViewDimension::D2Array),
        };

        let width = self.width;
        let height = self.height;

        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        };

        let mip_level_count = if let Some(max_mips) = self.with_mips {
            use std::cmp::{max, min};
            // // lod calcuation based on texture size clamp to max_mips
            let mip_count = min(max_mips, u32::ilog2(max(width, height)));
            trace!("Texture with mips: {}", mip_count);
            mip_count
        } else {
            1
        };

        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: self.label,
            size: extent,
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::from(self.format),
            usage: wgpu::TextureUsages::from(self.usage),
            view_formats: &[],
        });

        // ATTENTION:
        // format.target_pixel_byte_cost() is wrong
        // Despite being 4 bytes per pixel, these are 8 bytes per pixel in the table
        // Self::Rgba8Unorm
        // | Self::Rgba8UnormSrgb
        // | Self::Rgba8Snorm
        // | Self::Bgra8Unorm
        // | Self::Bgra8UnormSrgb
        // let pixel_size = format.target_pixel_byte_cost().unwrap_or(4);

        let pixel_size = self.format.pixel_size();
        let mut estimated_size = (width * height * layers * pixel_size) as usize;

        if let Some(source) = self.source {
            let pixels = match source {
                TextureSource::Cpu(data) => data.pixels.to_vec(),
                TextureSource::Static(data) => data.pixels.to_vec(),
            };

            let layer_size = (width * height * pixel_size) as usize;
            assert_eq!(
                pixels.len(),
                layer_size,
                "Data pixels does not match: layer_size"
            );

            // extend in case of Cube texture (layers = 6)
            let pixels: Vec<u8> = pixels
                .iter()
                .copied()
                .cycle()
                .take(layer_size * layers as usize)
                .collect();

            gpu.queue.write_texture(
                texture.as_image_copy(),
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pixel_size * width),
                    rows_per_image: Some(height),
                },
                extent,
            );

            // TODO: implement pixel size and with mips
            estimated_size = pixels.len();
        }

        let _sampler = match self.sampler {
            Some(sd) => gpu.device.create_sampler(&sd.into()),
            None => gpu.device.create_sampler(&Default::default()),
        };

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(view_dimension),
            mip_level_count: Some(1),
            ..Default::default()
        });

        let view_mips = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(view_dimension),
            mip_level_count: None, // all mips
            ..Default::default()
        });

        GpuTexture {
            extent,
            inner: Arc::new(texture),
            view: Arc::new(view),
            view_mips: Arc::new(view_mips),
            sampler: _sampler,
            estimated_size,
            format: self.format,
        }
    }
}
