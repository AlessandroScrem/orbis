use crate::ecs::entity_id::EntityId;
use crate::math::*;
use crate::renderer::uniform::LightUniform;

use super::*;

pub struct ShadowPass {}

const FIELDS: &[ResourceUse] = &[ResourceUse {
    resource: ResourceId::SHADOWMAP,
    access: ResourceAccess::Write,
}];

impl RenderPass for ShadowPass {
    fn name(&self) -> &'static str {
        "ShadowPass Opaque"
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
        let Some(lights) = frame.lights else {
            return;
        };

        for (slot, light) in lights
            .lights
            .iter()
            .enumerate()
            .filter(|(_, l)| l.cast_shadow.is_one())
            .take(lights.count as usize)
        {
            if let Some(shadow_view) = ctx.shadow_mgr.get_shadowmap_view(slot) {
                let size = size_of::<LightUniform>() as u64;
                let offset = slot as u64 * size;
                let source = ctx.buffer(BufferKind::Lights);
                let dest = ctx.shadow_mgr.get_buffer();
                encoder.copy_buffer_to_buffer(source, offset, dest, 0, size);

                build_shadowmap(encoder, ctx, frame, shadow_view);
                if let Some(entity) = frame.tasks.entity_selected
                    && EntityId(light.entity_id) == EntityId::from(entity)
                {
                    convert_texture(encoder, ctx, shadow_view);
                }
            }
        }
    }
}

fn build_shadowmap(
    encoder: &mut wgpu::CommandEncoder,
    ctx: &mut RenderContext,
    frame: &FrameData,
    view: &wgpu::TextureView,
) {
    let batches = &frame.opaque_batches;

    let bindgroup = ctx.shadow_mgr.get_bg();
    let pipeline = ctx.shadow_mgr.get_pipeline();

    let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Mesh Render Pass"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });

    renderpass.set_pipeline(pipeline);
    renderpass.set_bind_group(0, bindgroup, &[]);

    // -------------------------------------------------
    // INSTANCE BUFFER
    // -------------------------------------------------
    renderpass.set_vertex_buffer(1, ctx.buffer(BufferKind::Instances).slice(..));

    // -------------------------------------------------
    // DRAW
    // -------------------------------------------------
    for batch in batches {
        // ---------------------------------------------
        // GPU RESOURCES
        // ---------------------------------------------
        let Some(gpu_mesh) = ctx.gpu_cache.mesh.get(&batch.mesh) else {
            continue;
        };

        // ---------------------------------------------
        // GEOMETRY
        // ---------------------------------------------
        renderpass.set_vertex_buffer(0, gpu_mesh.vertexbuffer.slice(..));

        renderpass.set_index_buffer(gpu_mesh.indexbuffer.slice(..), IndexFormat::Uint32);

        // ---------------------------------------------
        // TRUE INSTANCING
        // ---------------------------------------------
        renderpass.draw_indexed(
            batch.submesh_index_range.clone(),
            0,
            batch.instance_start..batch.instance_start + batch.instance_count,
        );
    }
}

fn convert_texture(
    encoder: &mut wgpu::CommandEncoder,
    ctx: &mut RenderContext,
    shadow_view: &wgpu::TextureView,
) {
    let layout = ctx.bindgroup_layout(BindgroupLayoutKind::Depth);
    let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });

    let bindgroup = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadowmap_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
        ],
    });

    let view = &ctx.shadow_mgr.get_rgba().view;
    let pipeline_manager = ctx.pip_mgr;

    let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Convert Render Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
            resolve_target: None,
            depth_slice: None,
        })],
        ..Default::default()
    });

    let pipeline = pipeline_manager.get_render_pipeline(PipelineKind::Convert);

    renderpass.set_pipeline(pipeline);
    renderpass.set_bind_group(0, &bindgroup, &[]);

    renderpass.draw(0..3, 0..1);
}
