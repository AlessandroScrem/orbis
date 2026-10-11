use super::*;

pub struct OutlinePass {}

impl RenderPass for OutlinePass {
    fn name(&self) -> &'static str {
        "OutlinePass"
    }

    fn reads(&self) -> &[ResourceId] {
        &[ResourceId::ENTITY, ResourceId::LDR]
    }
    fn writes(&self) -> &[ResourceId] {
        &[ResourceId::LDR]
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        frame: &FrameData,
    ) {
        if frame.tasks.entity_selected.is_none() {
            return;
        }

        let frame_view = &ctx.target;
        let gpu_manager = ctx.gpu_mgr;
        let pipeline_manager = ctx.pip_mgr;

        // Render pass
        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Outline Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame_view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                resolve_target: None,
                depth_slice: None,
            })],
            ..Default::default()
        });

        let pipeline = pipeline_manager.get_render_pipeline(PipelineKind::Outline);

        renderpass.set_pipeline(&pipeline);
        renderpass.set_bind_group(
            0,
            ctx.framebuffer_bind_group(FramebufferKind::EntityId),
            &[],
        );
        renderpass.set_bind_group(1, gpu_manager.get_bindgroup(BindgroupKind::Perframe), &[]);
        renderpass.draw(0..3, 0..1);
    }
}
