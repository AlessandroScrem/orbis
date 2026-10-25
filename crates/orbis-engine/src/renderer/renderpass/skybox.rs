use super::*;

pub struct SkyboxPass {}

impl RenderPass for SkyboxPass {
    fn name(&self) -> &'static str {
        "SkyboxPass"
    }

    fn reads(&self) -> &[ResourceId] {
        &[ResourceId::HDR, ResourceId::DEPTH]
    }
    fn writes(&self) -> &[ResourceId] {
        &[ResourceId::HDR, ResourceId::DEPTH]
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        frame: &FrameData,
    ) {
        if !frame.tasks.skybox_enable {
            return;
        }

        let skybox_blur = frame.tasks.skybox_blur;
        let pipeline_manager = ctx.pip_mgr;

        // Render pass
        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Skybox Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx
                    .framebuffer_attachement_view(FramebufferKind::Hdr),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                resolve_target: None,
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.framebuffer_attachement_view(FramebufferKind::Depth),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        let pipeline = pipeline_manager.get_render_pipeline(PipelineKind::Skybox);
        let skybox_bind_group = ctx.ibl_skybox_bind_group(skybox_blur);

        renderpass.set_pipeline(pipeline);
        renderpass.set_bind_group(0, ctx.bindgroup(BindgroupKind::Perframe), &[]);
        renderpass.set_bind_group(1, skybox_bind_group, &[]);
        renderpass.draw(0..36, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    //Test single passes.
    #[test]
    fn skybox_passes_declare_expected_resource_access() {
        let pass = RenderPassEnum::Skybox(SkyboxPass{});
        
        assert_eq!(pass.reads(), &[ResourceId::HDR, ResourceId::DEPTH]);
        assert_eq!(
            pass.writes(),
            &[ResourceId::HDR, ResourceId::DEPTH]
        );
    }
}
