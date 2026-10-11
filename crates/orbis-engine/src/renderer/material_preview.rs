use crate::ResourceId;
use crate::assets::texture_asset::ColorSpace;
use crate::assets::{MaterialId, VertexInstance};
use crate::gpu::pipeline_manager::{PipelineKind, PipelineManager};
use crate::gpu::{BindgroupKind, BindgroupLayoutKind, GpuCache, GpuContext, GpuManager, GpuMesh};
use crate::math::{Deg, Mat4, Point3f, Vec3, perspective};
use cgmath::SquareMatrix;
use wgpu::util::DeviceExt;

struct MaterialPreviewTarget {
    target: std::sync::Arc<wgpu::Texture>,
    #[allow(dead_code)]
    target_view: std::sync::Arc<wgpu::TextureView>,
    #[allow(dead_code)]
    entity_target: wgpu::Texture,
    entity_view: wgpu::TextureView,
    #[allow(dead_code)]
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl MaterialPreviewTarget {
    const SIZE: u32 = 256;

    fn new(device: &wgpu::Device) -> Self {
        let target = std::sync::Arc::new(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Preview HDR"),
            size: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
        let target_view = std::sync::Arc::new(target.create_view(&Default::default()));

        let entity_target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Preview Entity"),
            size: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let entity_view = entity_target.create_view(&Default::default());

        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Preview Depth"),
            size: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());

        Self {
            target,
            target_view,
            entity_target,
            entity_view,
            depth,
            depth_view,
        }
    }

    fn texture(&self, device: &wgpu::Device) -> crate::gpu::GpuTexture {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        crate::gpu::GpuTexture {
            inner: self.target.clone(),
            view: self.target_view.clone(),
            view_mips: self.target_view.clone(),
            extent: wgpu::Extent3d {
                width: Self::SIZE,
                height: Self::SIZE,
                depth_or_array_layers: 1,
            },
            sampler,
            estimated_size: (Self::SIZE * Self::SIZE * 8) as usize,
            format: ColorSpace::Rgbaf16,
        }
    }
}

struct CachedMaterialPreview {
    material_revision: u64,
    environment_revision: u64,
    target: MaterialPreviewTarget,
}

pub struct MaterialPreviewRenderer {
    resource_id: ResourceId,
    cache: std::collections::HashMap<MaterialId, CachedMaterialPreview>,
    sphere: GpuMesh,
    index_count: u32,
    environment_revision: u64,
    active_material: Option<MaterialId>,
    displayed_material: Option<MaterialId>,
    camera_buffer: wgpu::Buffer,
    globals_buffer: wgpu::Buffer,
    lights_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    perframe_bind_group: Option<wgpu::BindGroup>,
}

fn create_preview_sphere(
    segments: u32,
    rings: u32,
) -> (Vec<crate::assets::MeshVertexData>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(((segments + 1) * (rings + 1)) as usize);
    let mut indices = Vec::with_capacity((segments * rings * 6) as usize);

    for y in 0..=rings {
        let v = y as f32 / rings as f32;
        let phi = std::f32::consts::PI * v;
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        for x in 0..=segments {
            let u = x as f32 / segments as f32;
            let theta = std::f32::consts::TAU * u;
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();

            let position = Vec3::new(sin_phi * cos_theta, cos_phi, sin_phi * sin_theta);
            let tangent = Vec3::new(-sin_theta, 0.0, cos_theta);

            vertices.push(crate::assets::MeshVertexData {
                position: position.into(),
                normal: position.into(),
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                uv: [u, 1.0 - v, 0.0, 0.0],
            });
        }
    }

    let stride = segments + 1;
    for y in 0..rings {
        for x in 0..segments {
            let i0 = y * stride + x;
            let i1 = i0 + 1;
            let i2 = i0 + stride;
            let i3 = i2 + 1;

            indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
        }
    }

    (vertices, indices)
}

impl MaterialPreviewRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let (vertices, indices) = create_preview_sphere(64, 32);
        let index_count = indices.len() as u32;
        let sphere = crate::gpu::GpuMesh::new(device, &vertices, &indices);

        let camera_buffer = create_preview_buffer(
            device,
            size_of::<crate::renderer::uniform::CameraUniform>(),
            "Material Preview Camera",
        );
        let globals_buffer = create_preview_buffer(
            device,
            size_of::<crate::renderer::uniform::GlobalUniform>(),
            "Material Preview Globals",
        );
        let lights_buffer = create_preview_buffer(
            device,
            size_of::<crate::renderer::uniform::LightsUniform>(),
            "Material Preview Lights",
        );
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Preview Instance"),
            contents: bytemuck::bytes_of(&VertexInstance::new(Mat4::identity(), 0)),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            resource_id: ResourceId::new(),
            cache: std::collections::HashMap::new(),
            sphere,
            index_count,
            environment_revision: 0,
            active_material: None,
            displayed_material: None,
            camera_buffer,
            globals_buffer,
            lights_buffer,
            instance_buffer,
            perframe_bind_group: None,
        }
    }

    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    pub fn invalidate_environment(&mut self) {
        self.environment_revision = self.environment_revision.wrapping_add(1);
    }

    pub fn set_material(&mut self, material: Option<MaterialId>) {
        self.active_material = material;
    }

    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        gpu_context: &GpuContext,
        gpu_manager: &GpuManager,
        gpu_cache: &GpuCache,
        pipeline_manager: &PipelineManager,
    ) -> Option<crate::gpu::GpuTexture> {
        let Some(material) = self.active_material else {
            self.active_material = None;
            return None;
        };
        let Some(gpu_material) = gpu_cache.material.get(&material) else {
            self.active_material = None;
            return None;
        };

        let revision = gpu_material.revision();
        if let Some(preview) = self.cache.get(&material)
            && preview.material_revision == revision
            && preview.environment_revision == self.environment_revision
        {
            if publish_cached_preview(&mut self.displayed_material, material) {
                return Some(preview.target.texture(&gpu_context.device));
            }
            return None;
        }

        let target = self
            .cache
            .remove(&material)
            .map(|preview| preview.target)
            .unwrap_or_else(|| MaterialPreviewTarget::new(&gpu_context.device));
        let material_bg = gpu_material.bind_group.as_ref()?;

        let view = Mat4::look_at_rh(
            Point3f::new(0.0, 0.0, 3.4),
            Point3f::new(0.0, 0.0, 0.0),
            Vec3::unit_y(),
        );
        let proj = perspective(Deg(35.0), 1.0, 0.1, 10.0);
        let camera = crate::renderer::uniform::CameraUniform {
            view_position: [0.0, 0.0, 3.4, 1.0],
            view: view.into(),
            proj: proj.into(),
            screen_size: [256.0, 256.0],
            ..Default::default()
        };

        let globals = crate::renderer::uniform::GlobalUniform {
            ibl_enable: 1,
            skybox_enable: 0,
            exposure: 0.0,
            ibl_intensity: 1.0,
            tonemap_filter: 0,
            ..Default::default()
        };

        let mut lights = crate::renderer::uniform::LightsUniform {
            enabled: 1,
            count: 1,
            ..Default::default()
        };
        lights.lights[0].color = [1.0, 1.0, 1.0];
        lights.lights[0].position = [2.0, 2.0, 3.0];
        lights.lights[0].directional = 0;
        lights.lights[0].cast_shadow = 0;       

        let instance = VertexInstance::new(Mat4::identity(), 0);
        gpu_context
            .queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        gpu_context
            .queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        gpu_context
            .queue
            .write_buffer(&self.lights_buffer, 0, bytemuck::bytes_of(&lights));
        gpu_context
            .queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::bytes_of(&instance));

        if self.perframe_bind_group.is_none() {
            let bind_group = gpu_context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: gpu_manager.get_bindgroup_layout(BindgroupLayoutKind::PerFrame),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.camera_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.globals_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.lights_buffer.as_entire_binding(),
                        },
                    ],
                    label: Some("Material Preview PerFrame Bind Group"),
                });
            self.perframe_bind_group = Some(bind_group);
        }
        let perframe_bind_group = self
            .perframe_bind_group
            .as_ref()
            .expect("preview PerFrame bind group must exist");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Material Preview Pass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.035,
                            g: 0.035,
                            b: 0.035,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.entity_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        pass.set_pipeline(pipeline_manager.get_render_pipeline(PipelineKind::Pbr));
        pass.set_bind_group(0, perframe_bind_group, &[]);
        pass.set_bind_group(1, material_bg, &[]);
        pass.set_bind_group(3, gpu_manager.get_bindgroup(BindgroupKind::PbrMap), &[]);
        pass.set_vertex_buffer(0, self.sphere.vertexbuffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.sphere.indexbuffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);

        drop(pass);

        let preview = target.texture(&gpu_context.device);
        self.cache.insert(
            material,
            CachedMaterialPreview {
                material_revision: revision,
                environment_revision: self.environment_revision,
                target,
            },
        );
        self.active_material = Some(material);
        self.displayed_material = Some(material);
        Some(preview)
    }
}

fn publish_cached_preview(displayed: &mut Option<MaterialId>, requested: MaterialId) -> bool {
    if *displayed == Some(requested) {
        return false;
    }

    *displayed = Some(requested);
    true
}

fn create_preview_buffer(device: &wgpu::Device, size: usize, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::publish_cached_preview;
    use crate::assets::MaterialId;

    #[test]
    fn cached_preview_is_published_when_selection_changes() {
        let first = MaterialId::new();
        let second = MaterialId::new();
        let mut displayed = None;

        assert!(publish_cached_preview(&mut displayed, first));
        assert_eq!(displayed, Some(first));
        assert!(!publish_cached_preview(&mut displayed, first));

        assert!(publish_cached_preview(&mut displayed, second));
        assert_eq!(displayed, Some(second));

        assert!(publish_cached_preview(&mut displayed, first));
        assert_eq!(displayed, Some(first));
        assert!(!publish_cached_preview(&mut displayed, first));
    }
}
