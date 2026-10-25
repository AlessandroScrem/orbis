use crate::gpu::pipeline_manager::PipelineManager;
use crate::gpu::{
    BindgroupKind, BindgroupLayoutKind, BufferKind, FramebufferKind, GpuCache, GpuContext,
    GpuManager, ShadowManager,
};
use crate::prelude::{debug, info};
use crate::renderer::framebuilder::{DrawStats, FrameData};
use crate::renderer::rendergraph::{CompiledRenderGraph, RenderGraph, ResourceId};
use crate::renderer::renderpass::*;
use wgpu::Device;

pub struct SceneRenderContext<'a> {
    pub gpu_context: &'a GpuContext,
    pub gpu_manager: &'a mut GpuManager,
    pub shadow_manager: &'a ShadowManager,
    pub pipeline_manager: &'a PipelineManager,
    pub gpu_cache: &'a GpuCache,
}

pub struct RenderContext<'a> {
    pub device: &'a Device,
    pub gpu_cache: &'a GpuCache,
    pub shadow_mgr: &'a ShadowManager,
    pub pip_mgr: &'a PipelineManager,
    pub target: &'a wgpu::TextureView,

    gpu_mgr: &'a GpuManager,
}

impl RenderContext<'_> {
    pub(crate) fn framebuffer_attachement_view(
        &self,
        kind: FramebufferKind,
    ) -> &wgpu::TextureView {
        self.gpu_mgr.get_framebuffer_view(kind)
    }

    pub(crate) fn framebuffer_texture(&self, kind: FramebufferKind) -> &wgpu::Texture {
        self.gpu_mgr.get_framebuffer_texture(kind)
    }

    pub(crate) fn framebuffer_sampler(&self, kind: FramebufferKind) -> &wgpu::Sampler {
        self.gpu_mgr.get_framebuffer_sampler(kind)
    }

    pub(crate) fn framebuffer_bind_group(&self, kind: FramebufferKind) -> &wgpu::BindGroup {
        self.gpu_mgr.get_framebuffer_bg(kind)
    }

    pub(crate) fn pbr_map_bind_group(&self) -> &wgpu::BindGroup {
        self.gpu_mgr.get_bindgroup(BindgroupKind::PbrMap)
    }

    pub(crate) fn buffer(&self, kind: BufferKind) -> &wgpu::Buffer {
        self.gpu_mgr.get_buffer(kind)
    }

    pub(crate) fn bindgroup_layout(&self, kind: BindgroupLayoutKind) -> &wgpu::BindGroupLayout {
        self.gpu_mgr.get_bindgroup_layout(kind)
    }

    pub(crate) fn bindgroup(&self, kind: BindgroupKind) -> &wgpu::BindGroup {
        assert!(
            !matches!(kind, BindgroupKind::PbrMap),
            "PbrMap bind group must use the dedicated accessor"
        );

        self.gpu_mgr.get_bindgroup(kind)
    }

    pub(crate) fn ibl_skybox_bind_group(&self, skybox_blur: bool) -> &wgpu::BindGroup {
        self.gpu_mgr.get_ibl_skybox_bg(skybox_blur)
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub struct FrameStats {
    pub opaque: DrawStats,
    pub transmission: DrawStats,
}

pub struct SceneRenderer {
    compiled_graph: CompiledRenderGraph,
    render_graph: RenderGraph,
    stats: FrameStats,
}

impl SceneRenderer {
    pub fn new() -> Self {
        let timer = std::time::Instant::now();
        info!("Initializing renderer...");

        debug!("Renderer initialized in {} ms", timer.elapsed().as_millis());

        let mut render_graph = RenderGraph::new();
        render_graph.add_pass(RenderPassEnum::Shadow(ShadowPass {}));
        render_graph.add_pass(RenderPassEnum::Mesh(MeshPass::opaque()));
        render_graph.add_pass(RenderPassEnum::Skybox(SkyboxPass {}));
        render_graph.add_pass(RenderPassEnum::BuildMipmaps(BuildMipmapsPass {}));
        render_graph.add_pass(RenderPassEnum::Transmission(MeshPass::transmission()));
        render_graph.add_pass(RenderPassEnum::LightsIcon(LightsIconPass {}));
        render_graph.add_pass(RenderPassEnum::Axis(AxisPass {}));
        render_graph.add_pass(RenderPassEnum::Lines(LinesPass {}));
        render_graph.add_pass(RenderPassEnum::Linearize(LinearizePass {}));
        render_graph.add_pass(RenderPassEnum::Outline(OutlinePass {}));

        let compiled_graph = render_graph
            .compile_persistent()
            .expect("the default render graph must compile");

        Self {
            render_graph,
            compiled_graph,
            stats: FrameStats::default(),
        }
    }

    pub fn get_render_stats(&self) -> FrameStats {
        self.stats
    }

    pub fn render(
        &mut self,
        runtime: &mut SceneRenderContext,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &FrameData,
    ) {
        let gpu_context = runtime.gpu_context;
        let gpu_manager = &mut *runtime.gpu_manager;
        let shadow_manager = runtime.shadow_manager;
        let pipeline_manager = runtime.pipeline_manager;
        let gpu_cache = runtime.gpu_cache;

        let framebuffer_lifetimes = self
            .compiled_graph
            .lifetimes()
            .iter()
            .filter_map(|lifetime| {
                let kind = match lifetime.resource {
                    ResourceId::HDR => FramebufferKind::Hdr,
                    ResourceId::OPAQUE => FramebufferKind::OpaqueWithMips,
                    ResourceId::ENTITY => FramebufferKind::EntityId,
                    ResourceId::DEPTH => FramebufferKind::Depth,
                    ResourceId::LDR | ResourceId::PICKBUFFER | ResourceId::SHADOWMAP => return None,
                };
                Some((kind, lifetime.first_use, lifetime.last_use))
            })
            .collect::<Vec<_>>();
        gpu_manager.prepare_transient_frame(&gpu_context.as_ref(), &framebuffer_lifetimes);

        let mut ctx = RenderContext {
            device: &gpu_context.device,
            gpu_cache,
            gpu_mgr: gpu_manager,
            shadow_mgr: shadow_manager,
            pip_mgr: pipeline_manager,
            target,
        };

        self.render_graph
            .execute(&self.compiled_graph, encoder, &mut ctx, frame);

        self.stats = FrameStats {
            opaque: frame.opaque_stats,
            transmission: frame.transmission_stats,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::SceneRenderer;

    #[test]
    fn default_graph_lifetimes_cover_transmission_and_outline_dependencies() {
        use crate::renderer::rendergraph::ResourceId;

        let renderer = SceneRenderer::new();
        let names = renderer.render_graph.compile_names().unwrap();
        let (_, lifetimes) = renderer.render_graph.compile_lifetimes().unwrap();
        let position = |name: &str| {
            names
                .iter()
                .position(|candidate| candidate.as_str() == name)
                .unwrap()
        };
        let lifetime = |resource| {
            lifetimes
                .iter()
                .find(|item| item.resource == resource)
                .unwrap()
        };

        let opaque = position("MeshPass Opaque");
        let transmission = position("MeshPass Transmission");
        let linearize = position("LinearizePass");
        let outline = position("OutlinePass");

        assert!(
            opaque < transmission,
            "opaque pass must precede transmission: {names:?}"
        );
        assert!(
            transmission < linearize,
            "transmission must precede linearization: {names:?}"
        );
        assert!(
            linearize < outline,
            "linearization must precede outline: {names:?}"
        );

        let hdr = lifetime(ResourceId::HDR);
        assert!(hdr.first_use <= opaque);
        assert!(hdr.last_use >= transmission);

        let entity = lifetime(ResourceId::ENTITY);
        assert!(entity.first_use <= opaque);
        assert!(entity.last_use >= outline);

        let ldr = lifetime(ResourceId::LDR);
        assert_eq!(ldr.first_use, linearize);
        assert_eq!(ldr.last_use, outline);
    }

}
