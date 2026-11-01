use super::*;

pub struct AxisPass {}

const FIELDS: &[ResourceUse] = &[
    ResourceUse {
        resource: ResourceId::HDR,
        access: ResourceAccess::ReadWrite,
    }
];

impl RenderPass for AxisPass {
    fn name(&self) -> &'static str {
        "AxisPass"
    }

    fn resources(&self) -> &[ResourceUse] {
        FIELDS
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        frame: &FrameData,
    ) {
        if !frame.tasks.axis_enable {
            return;
        }

        let pipeline_manager = ctx.pip_mgr;

        // Render pass
        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Axis Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.framebuffer_attachement_view(FramebufferKind::Hdr),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        let pipeline = pipeline_manager.get_render_pipeline(PipelineKind::Lines);

        renderpass.set_pipeline(pipeline);
        renderpass.set_bind_group(0, ctx.bindgroup(BindgroupKind::Camera), &[]);
        renderpass.set_vertex_buffer(0, ctx.buffer(BufferKind::Axis).slice(0..));
        renderpass.draw(0..6, 0..1);
    }
}
