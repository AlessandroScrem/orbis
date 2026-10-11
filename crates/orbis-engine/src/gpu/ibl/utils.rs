use crate::math::*;
use wgpu::{ShaderModule, util::DeviceExt};

#[repr(C, align(16))]
#[derive(Default, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Camera {
    pub view_proj: [[f32; 4]; 4],
}

/// Calculate the size of a mip level based on the original size and the mip level index.
/// # Arguments
/// * `source` - The original size of the texture (width or height).
/// * `mip_level` - The mip level index (0 for the original size, 1 for the first mip level, etc.).
/// # Returns
/// * The size of the texture at the specified mip level.
pub fn mip_size(source: u32, mip_level: u32) -> u32 {
    (((source as f32) * 0.5f32.powf(mip_level as f32)).floor() as u32).max(1)
}

/// Calculate the number of mip levels for a texture of a given size.
/// # Arguments
/// * `texture_size` - The size of the texture (width or height).
/// # Returns
/// * The number of mip levels.
pub fn mip_levels(texture_size: u32) -> u32 {
    (1.0 + (texture_size as f32).log2().floor()) as u32
}

/// Create a texture view for a specific face and mip level of a cubemap texture.
/// # Arguments
/// * `texture` - The cubemap texture.
/// * `face_index` - The index of the face (0-5 for a cubemap).
/// * `mip_level` - The mip level to create the view for.
/// # Returns
/// * A `wgpu::TextureView` for the specified face and mip level.
pub fn create_dest_view(
    texture: &wgpu::Texture,
    face_index: u32,
    mip_level: u32,
) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba16Float), // stesso formato della texture
        dimension: Some(wgpu::TextureViewDimension::D2),
        aspect: wgpu::TextureAspect::All,
        base_mip_level: mip_level, // mip level da usare
        mip_level_count: Some(1),  // solo 1 livello
        base_array_layer: face_index,
        array_layer_count: Some(1), // solo il primo layer
        ..Default::default()
    })
}

pub fn render_to_cubemap(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    frame_view: &wgpu::TextureView,
    capture_size: u32,
) {
    let clear_color = wgpu::Color::BLACK;

    let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Render Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: frame_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear_color),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    renderpass.set_viewport(0.0, 0.0, capture_size as f32, capture_size as f32, 0.0, 1.0);

    renderpass.set_pipeline(pipeline);
    renderpass.set_bind_group(0, bind_group, &[]);
    renderpass.draw(0..36, 0..1);
}

/// Create camera views for each face of a cubemap.
/// # Returns
/// * A vector of 6 `Matrix4<f32>` representing the view matrices for each cubemap face.
pub fn create_camera_views() -> Vec<Mat4> {
    const ZERO: Point3f = Point3f::new(0.0, 0.0, 0.0);
    const PX: [f32; 3] = [1.0, 0.0, 0.0];
    const NX: [f32; 3] = [-1.0, 0.0, 0.0];
    const PY: [f32; 3] = [0.0, 1.0, 0.0];
    const NY: [f32; 3] = [0.0, -1.0, 0.0];
    const PZ: [f32; 3] = [0.0, 0.0, 1.0];
    const NZ: [f32; 3] = [0.0, 0.0, -1.0];

    vec![
        // +X (right)
        Mat4::look_at_lh(ZERO, PX.into(), NY.into()),
        // -X (left)
        Mat4::look_at_lh(ZERO, NX.into(), NY.into()),
        // +Y (top)
        Mat4::look_at_lh(ZERO, PY.into(), PZ.into()),
        // -Y (bottom)
        Mat4::look_at_lh(ZERO, NY.into(), NZ.into()),
        // +Z (front)
        Mat4::look_at_lh(ZERO, PZ.into(), NY.into()),
        // -Z (back)
        Mat4::look_at_lh(ZERO, NZ.into(), NY.into()),
    ]
}

/// Create a camera uniform buffer.
/// # Arguments
/// * `device` - The wgpu device to create the buffer.
/// # Returns
/// * A `wgpu::Buffer` representing the camera uniform buffer.
pub fn create_camera_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Uniform Buffer"),
        contents: bytemuck::cast_slice(&[Camera::default()]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

/// Update the camera uniform buffer with the provided view matrix.
/// # Arguments
/// * `queue` - The wgpu queue to write the buffer.
/// * `camera_uniform_buffer` - The camera uniform buffer to update.
/// * `cam_view` - The view matrix to set in the camera uniform.
/// # Returns
/// * None.
pub fn update_camera_buffer(
    queue: &wgpu::Queue,
    camera_uniform_buffer: &wgpu::Buffer,
    cam_view: Mat4,
) {
    let cam_proj = perspective(Deg(90.0), 1.0, 0.1, 10.0);

    let updated_uniforms = Camera {
        view_proj: (cam_proj * cam_view).into(),
    };

    queue.write_buffer(
        camera_uniform_buffer,
        0,
        bytemuck::bytes_of(&updated_uniforms),
    );
}

pub fn create_cube_texture(
    device: &wgpu::Device,
    label: &str,
    size: u32,
    mip_level_count: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    let width = size;
    let height = size;
    let depth_or_array_layers = 6; // 6 faces
    create_texture(
        device,
        label,
        width,
        height,
        depth_or_array_layers,
        mip_level_count,
        format,
    )
}

pub fn create_texture(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    depth_or_array_layers: u32,
    mip_level_count: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers,
    };

    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent,
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    shader: ShaderModule,
    label: &str,
) -> wgpu::RenderPipeline {

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),

        primitive: Default::default(),
        multisample: Default::default(),
        layout: None,
        depth_stencil: None,
        cache: None,
        multiview_mask: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_calculate_mip_size() {
        assert_eq!(mip_size(300, 0), 300);
        assert_eq!(mip_size(300, 1), 150);
        assert_eq!(mip_size(300, 2), 75);
        assert_eq!(mip_size(300, 3), 37);
        assert_eq!(mip_size(300, 4), 18);
        assert_eq!(mip_size(300, 5), 9);
        assert_eq!(mip_size(300, 6), 4);
        assert_eq!(mip_size(300, 7), 2);
        assert_eq!(mip_size(300, 8), 1);
        assert_eq!(mip_size(300, 9), 1);
    }

    #[test]
    fn should_calculate_mip_levels() {
        assert_eq!(mip_levels(300), 9);
        assert_eq!(mip_levels(256), 9);
        assert_eq!(mip_levels(128), 8);
        assert_eq!(mip_levels(64), 7);
        assert_eq!(mip_levels(32), 6);
        assert_eq!(mip_levels(16), 5);
        assert_eq!(mip_levels(8), 4);
        assert_eq!(mip_levels(4), 3);
        assert_eq!(mip_levels(2), 2);
        assert_eq!(mip_levels(1), 1);
    }

    #[test]
    fn should_create_camera_views() {
        let views = create_camera_views();
        assert_eq!(views.len(), 6);
    }
}
