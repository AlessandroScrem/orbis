use super::*;

#[derive(PartialEq, Clone, Copy)]
pub enum MeshPassMode {
    Opaque,
    Transmission,
}

const OPAQUE_FIELDS: &[ResourceUse] = &[
    ResourceUse {
        resource: ResourceId::SHADOWMAP,
        access: ResourceAccess::Read,
    },
    ResourceUse {
        resource: ResourceId::HDR,
        access: ResourceAccess::Write,
    },
    ResourceUse {
        resource: ResourceId::ENTITY,
        access: ResourceAccess::Write,
    },
    ResourceUse {
        resource: ResourceId::DEPTH,
        access: ResourceAccess::Write,
    },
];

const TRANSMISSION_FIELDS: &[ResourceUse] = &[
    ResourceUse {
        resource: ResourceId::HDR,
        access: ResourceAccess::ReadWrite,
    },
    ResourceUse {
        resource: ResourceId::OPAQUE,
        access: ResourceAccess::Read,
    },
    ResourceUse {
        resource: ResourceId::ENTITY,
        access: ResourceAccess::ReadWrite,
    },
    ResourceUse {
        resource: ResourceId::DEPTH,
        access: ResourceAccess::ReadWrite,
    },
    ResourceUse {
        resource: ResourceId::SHADOWMAP,
        access: ResourceAccess::Read,
    },
];

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
    const HDR_CLEAR: wgpu::Color = wgpu::Color {
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 1.0,
    };
    const ENTITY_CLEAR: wgpu::Color = wgpu::Color::BLACK;
    const DEPTH_CLEAR: f32 = 1.0;

    pub fn opaque() -> Self {
        Self {
            config: MeshPassConfig {
                mode: MeshPassMode::Opaque,
                hdr_load: wgpu::LoadOp::Clear(Self::HDR_CLEAR),
                entity_load: wgpu::LoadOp::Clear(Self::ENTITY_CLEAR),
                depth_load: wgpu::LoadOp::Clear(Self::DEPTH_CLEAR),
            },
        }
    }
    pub fn transmission() -> Self {
        Self {
            config: MeshPassConfig {
                mode: MeshPassMode::Transmission,
                hdr_load: wgpu::LoadOp::Load,
                entity_load: wgpu::LoadOp::Load,
                depth_load: wgpu::LoadOp::Load,
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

    fn resources(&self) -> &[ResourceUse] {
        match self.config.mode {
            MeshPassMode::Opaque => OPAQUE_FIELDS,
            MeshPassMode::Transmission => TRANSMISSION_FIELDS,
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

        let pipeline_manager = ctx.pip_mgr;

        let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Mesh Render Pass"),
            color_attachments: &[
                // 0: opaque object
                Some(wgpu::RenderPassColorAttachment {
                    view: ctx.framebuffer_attachement_view(FramebufferKind::Hdr),
                    ops: wgpu::Operations {
                        load: self.config.hdr_load,
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                    depth_slice: None,
                }),
                // 1: entity ID
                Some(wgpu::RenderPassColorAttachment {
                    view: ctx.framebuffer_attachement_view(FramebufferKind::EntityId),
                    ops: wgpu::Operations {
                        load: self.config.entity_load,
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                    depth_slice: None,
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.framebuffer_attachement_view(FramebufferKind::Depth),
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
        renderpass.set_bind_group(0, ctx.bindgroup(BindgroupKind::Perframe), &[]);
        renderpass.set_bind_group(3, ctx.pbr_map_bind_group(), &[]);

        // -------------------------------------------------
        // INSTANCE BUFFER
        // -------------------------------------------------
        renderpass.set_vertex_buffer(1, ctx.buffer(BufferKind::Instances).slice(..));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_pass_clears_attachments() {
        let pass = MeshPass::opaque();
        let config = &pass.config;

        assert!(matches!(
            &config.hdr_load,
            wgpu::LoadOp::Clear(MeshPass::HDR_CLEAR)
        ));
        assert!(matches!(
            &config.entity_load,
            wgpu::LoadOp::Clear(MeshPass::ENTITY_CLEAR)
        ));
        assert!(matches!(
            &config.depth_load,
            wgpu::LoadOp::Clear(MeshPass::DEPTH_CLEAR)
        ));
    }

    #[test]
    fn transmission_pass_loads_attachments() {
        let pass = MeshPass::transmission();
        let config = &pass.config;

        assert!(matches!(&config.hdr_load, wgpu::LoadOp::Load));
        assert!(matches!(&config.entity_load, wgpu::LoadOp::Load));
        assert!(matches!(&config.depth_load, wgpu::LoadOp::Load));
    }
}
