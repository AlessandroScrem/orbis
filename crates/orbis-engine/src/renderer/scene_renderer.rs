use crate::gpu::pipeline_manager::PipelineManager;
use crate::gpu::{FramebufferKind, GpuCache, GpuContext, GpuManager, ShadowManager};
use crate::prelude::{debug, info};
use crate::renderer::framebuilder::{DrawStats, FrameData};
use crate::renderer::rendergraph::{RenderGraph, ResourceId};
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

    pub gpu_mgr: &'a GpuManager,
    pub shadow_mgr: &'a ShadowManager,
    pub pip_mgr: &'a PipelineManager,
    pub target: &'a wgpu::TextureView,
    active_pass_reads: Vec<ResourceId>,
    active_pass_writes: Vec<ResourceId>,
}

impl RenderContext<'_> {
    pub(crate) fn begin_pass(&mut self, reads: &[ResourceId], writes: &[ResourceId]) {
        self.active_pass_reads.clear();
        self.active_pass_reads.extend_from_slice(reads);
        self.active_pass_writes.clear();
        self.active_pass_writes.extend_from_slice(writes);
    }

    fn assert_framebuffer_access(&self, kind: FramebufferKind) {
        let resource = Self::resource_for_framebuffer(kind);
        assert!(
            Self::resource_is_declared(
                resource,
                &self.active_pass_reads,
                &self.active_pass_writes,
            ),
            "render pass accessed undeclared framebuffer resource {resource}"
        );
    }

    fn resource_is_declared(
        resource: ResourceId,
        reads: &[ResourceId],
        writes: &[ResourceId],
    ) -> bool {
        reads.contains(&resource) || writes.contains(&resource)
    }

    fn resource_for_framebuffer(kind: FramebufferKind) -> ResourceId {
        match kind {
            FramebufferKind::Hdr => ResourceId::HDR,
            FramebufferKind::OpaqueWithMips => ResourceId::OPAQUE,
            FramebufferKind::EntityId => ResourceId::ENTITY,
            FramebufferKind::Depth => ResourceId::DEPTH,
        }
    }

    pub(crate) fn framebuffer_view(&self, kind: FramebufferKind) -> &wgpu::TextureView {
        self.assert_framebuffer_access(kind);
        self.gpu_mgr.get_framebuffer_view(kind)
    }

    pub(crate) fn framebuffer_texture(&self, kind: FramebufferKind) -> &wgpu::Texture {
        self.assert_framebuffer_access(kind);
        self.gpu_mgr.get_framebuffer_texture(kind)
    }

    pub(crate) fn framebuffer_sampler(&self, kind: FramebufferKind) -> &wgpu::Sampler {
        self.assert_framebuffer_access(kind);
        self.gpu_mgr.get_framebuffer_sampler(kind)
    }

    pub(crate) fn framebuffer_bind_group(&self, kind: FramebufferKind) -> &wgpu::BindGroup {
        self.assert_framebuffer_access(kind);
        self.gpu_mgr.get_framebuffer_bg(kind)
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub struct FrameStats {
    pub opaque: DrawStats,
    pub transmission: DrawStats,
}

pub struct SceneRenderer {
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

        Self {
            render_graph,
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

        let framebuffer_lifetimes = match self.render_graph.compile_lifetimes() {
            Ok((_, lifetimes)) => lifetimes
                .into_iter()
                .filter_map(|lifetime| {
                    let kind = match lifetime.resource {
                        ResourceId::HDR => FramebufferKind::Hdr,
                        ResourceId::OPAQUE => FramebufferKind::OpaqueWithMips,
                        ResourceId::ENTITY => FramebufferKind::EntityId,
                        ResourceId::DEPTH => FramebufferKind::Depth,
                        ResourceId::LDR | ResourceId::PICKBUFFER | ResourceId::SHADOWMAP => {
                            return None;
                        }
                    };
                    Some((kind, lifetime.first_use, lifetime.last_use))
                })
                .collect::<Vec<_>>(),
            Err(error) => {
                log::error!("Render graph compilation failed; frame rendering skipped: {error}");
                return;
            }
        };
        gpu_manager.prepare_transient_frame(&gpu_context.as_ref(), &framebuffer_lifetimes);

        let mut ctx = RenderContext {
            device: &gpu_context.device,
            gpu_cache,
            gpu_mgr: gpu_manager,
            shadow_mgr: shadow_manager,
            pip_mgr: pipeline_manager,
            target,
            active_pass_reads: Vec::new(),
            active_pass_writes: Vec::new(),
        };

        if let Err(error) = self.render_graph.execute(encoder, &mut ctx, frame) {
            log::error!("Render graph compilation failed; frame rendering skipped: {error}");
            return;
        }

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
        let position = |name: &str| names.iter().position(|candidate| candidate.as_str() == name).unwrap();
        let lifetime = |resource| lifetimes.iter().find(|item| item.resource == resource).unwrap();

        let opaque = position("MeshPass Opaque");
        let transmission = position("MeshPass Transmission");
        let linearize = position("LinearizePass");
        let outline = position("OutlinePass");

        assert!(opaque < transmission, "opaque pass must precede transmission: {names:?}");
        assert!(transmission < linearize, "transmission must precede linearization: {names:?}");
        assert!(linearize < outline, "linearization must precede outline: {names:?}");

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

    #[test]
    fn default_render_graph_compiles_in_pass_order() {
        let renderer = SceneRenderer::new();
        let order = renderer.render_graph.compile_names().unwrap();

        assert_eq!(
            order,
            vec![
                "ShadowPass Opaque",
                "MeshPass Opaque",
                "SkyboxPass",
                "BuildMipmapsPass",
                "MeshPass Transmission",
                "LightPass",
                "AxisPass",
                "BoundingboxPass",
                "LinearizePass",
                "OutlinePass",
            ]
        );
    }
}


#[cfg(test)]
mod render_context_tests {
    use super::*;

    #[test]
    fn framebuffer_kinds_map_to_their_graph_resources() {
        assert_eq!(RenderContext::resource_for_framebuffer(FramebufferKind::Hdr), ResourceId::HDR);
        assert_eq!(
            RenderContext::resource_for_framebuffer(FramebufferKind::OpaqueWithMips),
            ResourceId::OPAQUE
        );
        assert_eq!(
            RenderContext::resource_for_framebuffer(FramebufferKind::EntityId),
            ResourceId::ENTITY
        );
        assert_eq!(RenderContext::resource_for_framebuffer(FramebufferKind::Depth), ResourceId::DEPTH);
    }

    #[test]
    fn declared_reads_and_writes_allow_framebuffer_access() {
        assert!(RenderContext::resource_is_declared(
            ResourceId::HDR,
            &[ResourceId::HDR],
            &[],
        ));
        assert!(RenderContext::resource_is_declared(
            ResourceId::ENTITY,
            &[],
            &[ResourceId::ENTITY],
        ));
    }

    #[test]
    fn undeclared_framebuffer_access_is_rejected() {
        assert!(!RenderContext::resource_is_declared(
            ResourceId::DEPTH,
            &[ResourceId::HDR],
            &[ResourceId::ENTITY],
        ));
    }
}
