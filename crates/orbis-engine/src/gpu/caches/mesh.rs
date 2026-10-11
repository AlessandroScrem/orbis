use std::collections::HashMap;

use crate::ResourceId;
use crate::{
    assets::MeshVertexData,
    gpu::{GpuResourceStats, HasGpuStats},
};

use wgpu::util::DeviceExt;

#[derive(Default)]
pub struct GpuMeshCache {
    map: HashMap<ResourceId, GpuMesh>,
    stats: GpuResourceStats,
}

impl HasGpuStats for GpuMeshCache {
    fn get_stats(&self) -> GpuResourceStats {
        self.stats.clone()
    }
}

impl GpuMeshCache {
    pub fn insert(&mut self, id: ResourceId, gpu_mesh: GpuMesh) {
        if let Some(previous) = self.map.insert(id, gpu_mesh) {
            self.stats.remove(previous.estimated_size);
        }
        self.stats.add(self.map[&id].estimated_size);
    }

    pub fn get(&self, id: &ResourceId) -> Option<&GpuMesh> {
        self.map.get(id)
    }

    pub fn remove(&mut self, id: ResourceId) {
        if let Some(gpu_mesh) = self.map.remove(&id) {
            self.stats.remove(gpu_mesh.estimated_size);
        }
    }
}

pub struct GpuMesh {
    pub vertexbuffer: wgpu::Buffer,
    pub indexbuffer: wgpu::Buffer,
    _indexcount: u32,
    estimated_size: usize,
}

impl GpuMesh {
    pub fn new(
        device: &wgpu::Device,
        vertices: &[MeshVertexData],
        indices: &[u32],
    ) -> GpuMesh {
        let vertexbuffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let indexbuffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let indexcount = indices.len() as u32;
        let estimated_size =
            std::mem::size_of_val(vertices) + std::mem::size_of_val(indices);

        GpuMesh {
            vertexbuffer,
            indexbuffer,
            _indexcount: indexcount,
            estimated_size,
        }
    }
}
