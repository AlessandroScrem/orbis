use super::*;

pub struct LinearizePass {}

impl RenderPass for LinearizePass {
    fn name(&self) -> &'static str {
        "LinearizePass"
    }

    fn reads(&self) -> &[ResourceId] {
        &[ResourceId::HDR]
    }
    fn writes(&self) -> &[ResourceId] {
        &[ResourceId::LDR]
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        _frame: &FrameData,
    ) {
        let gpu_manager = ctx.gpu_mgr;
        let pipeline_manager = ctx.pip_mgr;
        let frame_view = &ctx.target;

        // Render pass
        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Hdr Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            ..Default::default()
        });

        let pipeline = pipeline_manager.get_render_pipeline(PipelineKind::Hdr);

        renderpass.set_pipeline(&pipeline);
        renderpass.set_bind_group(0, ctx.framebuffer_bind_group(FramebufferKind::Hdr), &[]);
        renderpass.set_bind_group(1, gpu_manager.get_bindgroup(BindgroupKind::Perframe), &[]);
        renderpass.draw(0..3, 0..1);
    }
}
