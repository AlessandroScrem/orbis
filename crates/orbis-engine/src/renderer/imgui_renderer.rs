use crate::ResourceId;
use crate::asset_path;
use crate::gpu::*;
use crate::prelude::*;
use crate::ui::UiTextures;
use imgui_wgpu::*;
use std::collections::HashMap;
use wgpu::*;

pub struct ImGuiTextureRegistry {
    textures: HashMap<ResourceId, imgui::TextureId>,
}

impl ImGuiTextureRegistry {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

    pub fn add(
        &mut self,
        renderer: &mut imgui_wgpu::Renderer,
        device: &Device,
        resource_id: ResourceId,
        texture: &GpuTexture,
    ) -> imgui::TextureId {
        let texture_config = RawTextureConfig {
            label: None,
            sampler_desc: wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            },
        };
        let imgui_texture = imgui_wgpu::Texture::from_raw_parts(
            device,
            renderer,
            texture.inner.clone(),
            texture.view.clone(),
            None,
            Some(&texture_config),
            texture.extent,
        );

        if let Some(id) = self.textures.get(&resource_id).copied() {
            renderer.textures.replace(id, imgui_texture);
            id
        } else {
            let id = renderer.textures.insert(imgui_texture);
            self.textures.insert(resource_id, id);
            id
        }
    }

    pub fn remove(
        &mut self,
        renderer: &mut imgui_wgpu::Renderer,
        resource_id: ResourceId,
    ) -> Option<imgui::TextureId> {
        let id = self.textures.remove(&resource_id)?;
        renderer.textures.remove(id);
        Some(id)
    }

    #[allow(unused)]
    pub fn get(&self, resource_id: ResourceId) -> Option<imgui::TextureId> {
        self.textures.get(&resource_id).copied()
    }

    pub fn ui_textures(&self, shadow_map: ResourceId, material_preview: ResourceId) -> UiTextures {
        UiTextures::new(
            self.textures.clone(),
            self.textures.get(&shadow_map).copied(),
            self.textures.get(&material_preview).copied(),
        )
    }
}


pub struct ImguiRender {
    pub renderer: imgui_wgpu::Renderer,
    pub registry: ImGuiTextureRegistry,
}

impl ImguiRender {
    pub fn new(
        device: &Device,
        queue: &Queue,
        window: &winit::window::Window,
        context: &mut imgui::Context,
        texture_format: wgpu::TextureFormat,
    ) -> Self {
        let renderer_config = RendererConfig {
            texture_format,
            ..Default::default()
        };

        let hidpi_factor = window.scale_factor();
        println!("Scale factor {}", hidpi_factor);
        let font_size = (9.0 * hidpi_factor) as f32;

        context.fonts().add_font(&[
            imgui::FontSource::DefaultFontData {
                config: Some(imgui::FontConfig {
                    name: Some("Default".into()),
                    oversample_h: 1,
                    pixel_snap_h: true,
                    size_pixels: font_size,
                    ..Default::default()
                }),
            },
            imgui::FontSource::TtfData {
                data: include_bytes!(asset_path!("fonts/codicon.ttf")),
                size_pixels: 9.0,
                config: Some(imgui::FontConfig {
                    name: Some("Codicons".into()),
                    pixel_snap_h: true,
                    oversample_h: 1,
                    glyph_ranges: imgui::FontGlyphRanges::from_slice(&[0xEA60, 0xEC1E, 0]),
                    ..imgui::FontConfig::default()
                }),
            },
        ]);

        let renderer = imgui_wgpu::Renderer::new(context, device, queue, renderer_config);
        let registry = ImGuiTextureRegistry::new();

        Self { renderer, registry }
    }

    pub fn render(
        &mut self,
        draw_data: &imgui::DrawData,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        device: &Device,
        queue: &Queue,
    ) {
        let frame_view = target;

        let mut pass = {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ImGui Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            })
        };

        match self.renderer.render(draw_data, queue, device, &mut pass) {
            Ok(()) => {}
            Err(e) => {
                error!("Imgui Render failed: {:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::{GpuTextureBuilder, GpuTextureUsage};
    use crate::test_utils;
    use std::sync::Mutex;

    static IMGUI_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn create_renderer(
        context: &mut imgui::Context,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> imgui_wgpu::Renderer {
        imgui_wgpu::Renderer::new(
            context,
            device,
            queue,
            imgui_wgpu::RendererConfig {
                texture_format: wgpu::TextureFormat::Rgba8Unorm,
                ..Default::default()
            },
        )
    }

    #[test]
    fn replacing_texture_preserves_registry_id() {
        let _guard = IMGUI_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let gpu = test_utils::get_gpu_context_test();
        let mut context = imgui::Context::create();
        let mut renderer = create_renderer(&mut context, gpu.device, gpu.queue);
        let mut registry = ImGuiTextureRegistry::new();
        let resource_id = ResourceId::new();
        let first = GpuTextureBuilder::from_empty(1, 1)
            .usage(GpuTextureUsage::RenderTarget)
            .build(&gpu);
        let second = GpuTextureBuilder::from_empty(2, 2)
            .usage(GpuTextureUsage::RenderTarget)
            .build(&gpu);

        let first_id = registry.add(&mut renderer, gpu.device, resource_id, &first);
        let second_id = registry.add(&mut renderer, gpu.device, resource_id, &second);

        assert_eq!(second_id, first_id);
        assert_eq!(registry.get(resource_id), Some(first_id));
        assert_eq!(registry.remove(&mut renderer, resource_id), Some(first_id));
    }

    #[test]
    fn remove_returns_registered_texture_id() {
        let _guard = IMGUI_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let gpu = test_utils::get_gpu_context_test();
        let mut context = imgui::Context::create();
        let mut renderer = create_renderer(&mut context, gpu.device, gpu.queue);
        let mut registry = ImGuiTextureRegistry::new();
        let texture = GpuTextureBuilder::from_empty(1, 1)
            .usage(GpuTextureUsage::RenderTarget)
            .build(&gpu);
        let resource_id = ResourceId::new();

        let texture_id = registry.add(&mut renderer, gpu.device, resource_id, &texture);

        assert_eq!(
            registry.remove(&mut renderer, resource_id),
            Some(texture_id)
        );
        assert_eq!(registry.get(resource_id), None);
        assert_eq!(registry.remove(&mut renderer, resource_id), None);
    }
}
