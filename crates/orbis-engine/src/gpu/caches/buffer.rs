use crate::{
    assets::VertexInstance,
    renderer::uniform::{CameraUniform, GlobalUniform, LightUniform, LightsUniform},
};

use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use wgpu::util::DeviceExt;

use crate::assets::LinesVertexData;
pub const MAX_INSTANCES: usize = 1000;
pub const MAX_LINE_VERTICES: usize = 100000;

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

type AxisData = [LinesVertexData; 6];

#[rustfmt::skip] 
pub static AXIS_VERICES: AxisData = [
    LinesVertexData{position: [0.0, 0.0, 0.0], color: RED},
    LinesVertexData{position: [10.0, 0.0, 0.0], color: RED},   //X  
    LinesVertexData{position: [0.0, 0.0, 0.0], color: GREEN},
    LinesVertexData{position: [0.0, 10.0, 0.0], color: GREEN}, //Y
    LinesVertexData{position: [0.0, 0.0, 0.0], color: BLUE},
    LinesVertexData{position: [0.0, 0.0, 10.0], color: BLUE},  //Z
];

#[derive(Debug, Clone, Copy, EnumIter)]
pub enum BufferKind {
    Camera,
    Globals,
    Lights,
    Light,
    Axis,
    Instances,
    Lines,
}

impl BufferKind {
    pub const fn buffer_size(kind: BufferKind) -> usize {
        match kind {
            BufferKind::Instances => size_of::<VertexInstance>() * MAX_INSTANCES,
            BufferKind::Lines => size_of::<LinesVertexData>() * MAX_LINE_VERTICES,
            BufferKind::Camera => size_of::<CameraUniform>(),
            BufferKind::Globals => size_of::<GlobalUniform>(),
            BufferKind::Lights => size_of::<LightsUniform>(),
            BufferKind::Light => size_of::<LightUniform>(),
            BufferKind::Axis => size_of::<AxisData>(),
        }
    }
}

pub struct BufferCache {
    buffers: Vec<wgpu::Buffer>,
}

impl BufferCache {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer: Vec<wgpu::Buffer> = BufferKind::iter()
            .map(|kind| Self::create(device, kind))
            .collect();
        Self { buffers: buffer }
    }

    pub fn get(&self, kind: BufferKind) -> &wgpu::Buffer {
        &self.buffers[kind as usize]
    }

    pub fn write<T: bytemuck::Pod>(&self, queue: &wgpu::Queue, kind: BufferKind, data: &[T]) {
        let byte_len = std::mem::size_of_val(data);
        let buffer = &self.buffers[kind as usize];

        assert!(
            byte_len <= BufferKind::buffer_size(kind),
            "Data size exceeds buffer:{:?} size",
            kind
        );

        queue.write_buffer(buffer, 0, bytemuck::cast_slice(data));
    }
}

impl BufferCache {
    fn create(device: &wgpu::Device, kind: BufferKind) -> wgpu::Buffer {
        match kind {
            BufferKind::Camera => device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Camera Uniform Buffer"),
                contents: bytemuck::cast_slice(&[CameraUniform::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            BufferKind::Globals => device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Globals Uniform Buffer"),
                contents: bytemuck::cast_slice(&[GlobalUniform::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            BufferKind::Axis => device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("LineAxis Vertex Buffer"),
                contents: bytemuck::cast_slice(&AXIS_VERICES),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            BufferKind::Lights => device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Lights Uniform Buffer"),
                contents: bytemuck::cast_slice(&[LightsUniform::default()]),
                usage: wgpu::BufferUsages::UNIFORM
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            }),
            BufferKind::Light => device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Light Uniform Buffer"),
                contents: bytemuck::cast_slice(&[LightUniform::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            BufferKind::Instances => device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer"),
                size: (std::mem::size_of::<VertexInstance>() * MAX_INSTANCES) as u64, // TODO! dynamic
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            BufferKind::Lines => device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Lines Vertex Buffer"),
                size: (std::mem::size_of::<LinesVertexData>() * MAX_LINE_VERTICES) as u64, // TODO! dynamic
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
}
