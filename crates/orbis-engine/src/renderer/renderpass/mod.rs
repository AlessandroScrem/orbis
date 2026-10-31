mod axis;
mod build_mipmaps;
mod light_icon;
mod linearize;
mod lines;
mod mesh;
mod outline;
mod shadow_map;
mod skybox;

pub(crate) use axis::*;
pub(crate) use build_mipmaps::*;
pub(crate) use light_icon::*;
pub(crate) use linearize::*;
pub(crate) use lines::*;
pub(crate) use mesh::*;
pub(crate) use outline::*;
pub(crate) use shadow_map::*;
pub(crate) use skybox::*;

use crate::gpu::pipeline_manager::PipelineKind;
use crate::renderer::FrameData;
use crate::renderer::scene_renderer::RenderContext;

use crate::gpu::caches::*;
use wgpu::IndexFormat;

use super::rendergraph::{ResourceUse, ResourceId, ResourceAccess};
use crate::assets::MaterialId;

pub(crate) trait RenderPass {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    fn resources(&self) -> &[ResourceUse];

    fn execute(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        _ctx: &mut RenderContext,
        _frame: &FrameData,
    ) {
    }
}

pub(crate) enum RenderPassEnum {
    Shadow(ShadowPass),
    Mesh(MeshPass),
    Transmission(MeshPass),
    BuildMipmaps(BuildMipmapsPass),
    LightsIcon(LightsIconPass),
    Skybox(SkyboxPass),
    Axis(AxisPass),
    Lines(LinesPass),
    Linearize(LinearizePass),
    Outline(OutlinePass),
}

macro_rules! impl_render_pass_enum {
    ($self:ident, $method:ident $(, $arg:ident)*) => {
        match $self {
            Self::Shadow(p) => p.$method($($arg),*),
            Self::Mesh(p) => p.$method($($arg),*),
            Self::Transmission(p) => p.$method($($arg),*),
            Self::Skybox(p) => p.$method($($arg),*),
            Self::BuildMipmaps(p) => p.$method($($arg),*),
            Self::LightsIcon(p) => p.$method($($arg),*),
            Self::Axis(p) => p.$method($($arg),*),
            Self::Lines(p) => p.$method($($arg),*),
            Self::Linearize(p) => p.$method($($arg),*),
            Self::Outline(p) => p.$method($($arg),*),
        }
    };
}

impl RenderPass for RenderPassEnum {
    fn name(&self) -> &'static str {
        impl_render_pass_enum!(self, name)
    }

    fn resources(&self) -> &[ResourceUse] {
        impl_render_pass_enum!(self, resources)
    }


    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        frame: &FrameData,
    ) {
        impl_render_pass_enum!(self, execute, encoder, ctx, frame)
    }
}
