use super::*;

use std::sync::Arc;
use wgpu::*;
use winit::window::Window;

pub struct GpuSurface {
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
}

impl GpuSurface {
    pub fn new(adapter: &Adapter, instance: &Instance, window: Arc<Window>) -> Self {
        info!("Initializing Surface...");
        let surface = instance
            .create_surface(window.clone())
            .expect("unable to create Surface");
        let surface_caps = surface.get_capabilities(adapter);
        let surface_format = surface_caps.formats[0];
        let size = window.inner_size();

        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: PresentMode::AutoNoVsync,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        info!(
            "Gpu Surface created:  format is {:?}",
            surface_config.format
        );

        Self {
            surface,
            surface_config,
        }
    }

    pub fn get_config(&self) -> &SurfaceConfiguration {
        &self.surface_config
    }

    pub fn get_frame(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture) => Some(texture),
            CurrentSurfaceTexture::Suboptimal(texture) => Some(texture),
            _ => None,
        }
    }

    pub fn resize_frame(&mut self, device: &Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.surface_config.width = width;
        self.surface_config.height = height;

        self.surface.configure(device, &self.surface_config);
    }
}
