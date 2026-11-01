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
