use super::*;

pub enum MeshPassMode {
    Opaque,
    Transmission,
}

pub struct MeshPassConfig {
    pub hdr_load: wgpu::LoadOp<wgpu::Color>,
    pub entity_load: wgpu::LoadOp<wgpu::Color>,
    pub depth_load: wgpu::LoadOp<f32>,
    pub mode: MeshPassMode,
}

pub struct MeshPass {
    config: MeshPassConfig,
}

impl MeshPass {
    pub fn opaque() -> Self {
        let clear_color = wgpu::Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 1.0,
        };
        Self {
            config: MeshPassConfig {
                hdr_load: wgpu::LoadOp::Clear(clear_color),
                entity_load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                depth_load: wgpu::LoadOp::Clear(1.0),
                mode: MeshPassMode::Opaque,
            },
        }
    }
    pub fn transmission() -> Self {
        Self {
            config: MeshPassConfig {
                hdr_load: wgpu::LoadOp::Load,
                entity_load: wgpu::LoadOp::Load,
                depth_load: wgpu::LoadOp::Load,
                mode: MeshPassMode::Transmission,
            },
        }
    }
}

impl RenderPass for MeshPass {
    fn name(&self) -> &'static str {
        match self.config.mode {
            MeshPassMode::Opaque => "MeshPass Opaque",
            MeshPassMode::Transmission => "MeshPass Transmission",
        }
    }

    fn reads(&self) -> &[ResourceId] {
        match self.config.mode {
            MeshPassMode::Opaque => &[ResourceId::SHADOWMAP],
            MeshPassMode::Transmission => &[
                ResourceId::HDR,
                ResourceId::OPAQUE,
                ResourceId::ENTITY,
                ResourceId::DEPTH,
                ResourceId::SHADOWMAP,
            ],
        }
    }
    fn writes(&self) -> &[ResourceId] {
        match self.config.mode {
            MeshPassMode::Opaque => &[
                ResourceId::HDR,
                ResourceId::OPAQUE,
                ResourceId::ENTITY,
                ResourceId::DEPTH,
            ],
            MeshPassMode::Transmission => &[ResourceId::HDR, ResourceId::ENTITY, ResourceId::DEPTH],
        }
    }

    fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        frame: &FrameData,
    ) {
        let batches = match self.config.mode {
            MeshPassMode::Opaque => &frame.opaque_batches,
            MeshPassMode::Transmission => &frame.transmission_batches,
        };

        let gpu_manager = ctx.gpu_mgr;
        let pipeline_manager = ctx.pip_mgr;

        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Mesh Render Pass"),
            color_attachments: &[
                // 0: opaque object
                Some(wgpu::RenderPassColorAttachment {
                    view: ctx.framebuffer_view(FramebufferKind::Hdr),
                    ops: wgpu::Operations {
                        load: self.config.hdr_load,
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                    depth_slice: None,
                }),
                // 1: entity ID
                Some(wgpu::RenderPassColorAttachment {
                    view: ctx.framebuffer_view(FramebufferKind::EntityId),
                    ops: wgpu::Operations {
                        load: self.config.entity_load,
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                    depth_slice: None,
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.framebuffer_view(FramebufferKind::Depth),
                depth_ops: Some(wgpu::Operations {
                    load: self.config.depth_load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        let render_pipeline = pipeline_manager.get_render_pipeline(PipelineKind::Pbr);

        renderpass.set_pipeline(render_pipeline);
        renderpass.set_bind_group(0, gpu_manager.get_bindgroup(BindgroupKind::Perframe), &[]);
        renderpass.set_bind_group(3, gpu_manager.get_bindgroup(BindgroupKind::PbrMap), &[]);

        // -------------------------------------------------
        // INSTANCE BUFFER
        // -------------------------------------------------
        renderpass.set_vertex_buffer(1, gpu_manager.get_buffer(BufferKind::Instances).slice(..));

        // -------------------------------------------------
        // SORT BY MATERIAL
        // -------------------------------------------------
        let mut sorted_batches: Vec<_> = batches.iter().collect();
        sorted_batches.sort_by_key(|b| b.material.raw());

        let mut current_material: Option<MaterialId> = None;
        // -------------------------------------------------
        // DRAW
        // -------------------------------------------------
        for batch in sorted_batches {
            // ---------------------------------------------
            // GPU RESOURCES
            // ---------------------------------------------
            let Some(gpu_mesh) = ctx.gpu_cache.mesh.get(&batch.mesh) else {
                continue;
            };
            let Some(gpu_material) = ctx.gpu_cache.material.get(&batch.material) else {
                continue;
            };
            let Some(material_bg) = gpu_material.bind_group.as_ref() else {
                continue;
            };

            // ---------------------------------------------
            // BIND MATERIAL ONLY IF CHANGED
            // ---------------------------------------------
            if current_material != Some(batch.material) {
                renderpass.set_bind_group(1, material_bg, &[]);
                current_material = Some(batch.material);
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
}
