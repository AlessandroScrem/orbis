use super::*;

pub struct LightsIconPass {}

const FIELDS: &[ResourceUse] = &[
    ResourceUse {
        resource: ResourceId::HDR,
        access: ResourceAccess::ReadWrite,
    },
    ResourceUse {
        resource: ResourceId::ENTITY,
        access: ResourceAccess::Read,
    },
    ResourceUse {
        resource: ResourceId::DEPTH,
        access: ResourceAccess::ReadWrite,
    },
];

impl RenderPass for LightsIconPass {
    fn name(&self) -> &'static str {
        "LightPass"
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
        if let Some(light_uniform) = frame.lights {
            if light_uniform.enabled == 0 {
                return;
            }

            let pipeline_manager = ctx.pip_mgr;

            // Render pass
            let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("LightIcon Render Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: ctx.framebuffer_attachement_view(FramebufferKind::Hdr),
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    }),
                    // 1: entity ID
                    Some(wgpu::RenderPassColorAttachment {
                        view: ctx.framebuffer_attachement_view(FramebufferKind::EntityId),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        resolve_target: None,
                        depth_slice: None,
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: ctx.framebuffer_attachement_view(FramebufferKind::Depth),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let pipeline = pipeline_manager.get_render_pipeline(PipelineKind::LightIcon);
            let perframe_bg = ctx.bindgroup(BindgroupKind::Perframe);
            let light_bg = ctx.bindgroup(BindgroupKind::LightIcon);

            renderpass.set_pipeline(pipeline);
            renderpass.set_bind_group(0, perframe_bg, &[]);
            renderpass.set_bind_group(1, light_bg, &[]);
            renderpass.draw(0..6, 0..light_uniform.count);
        }
    }
}
