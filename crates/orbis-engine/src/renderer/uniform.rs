use crate::camera::Camera;
use crate::globals::Globals;
use crate::math::*;

///shader: [pbr, blinnphong, equirectangular_to_cubemap, irradiance_convolution, light, lines, prefilter_map, skybox]
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_position: [f32; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub screen_size: [f32; 2],
    pub _pad: [f32; 2],
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            view_position: [0f32; 4],
            view: [[0f32; 4]; 4],
            proj: [[0f32; 4]; 4],
            screen_size: [1.0f32; 2],
            _pad: [0f32; 2],
        }
    }
}

impl CameraUniform {
    pub fn from_camera_size(camera: &Camera, size: (u32, u32)) -> Self {
        let screen_size = [size.0 as f32, size.1 as f32];
        Self {
            view_position: camera.get_position().to_homogeneous().into(),
            view: camera.get_view_mat().into(),
            proj: camera.get_projection_mat().into(),
            screen_size,
            ..Default::default()
        }
    }
}

impl GlobalUniform {
    pub fn from_global_id(globals: &Globals, entity_id: u64) -> Self {
        Self {
            ibl_enable: globals.ibl_enable as u32,
            skybox_enable: globals.skybox_enable as u32,
            exposure: globals.exposure,
            ibl_intensity: globals.ibl_intensity,
            tonemap_filter: globals.tonemap_filter,
            entity_id,
            debug: globals.debug_code,
            env_rotation: globals.env_rotation.to_radians(),
            ..Default::default()
        }
    }
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Mat3Std140 {
    m: [[f32; 4]; 3],
}

impl Default for Mat3Std140 {
    fn default() -> Self {
        Self::identity()
    }
}

impl From<Mat3Std140> for [[f32; 4]; 3] {
    fn from(val: Mat3Std140) -> Self {
        val.m
    }
}

impl Mat3Std140 {
    pub fn mat3_to_std140(n: Mat3) -> Self {
        Self {
            m: [
                [n.x.x, n.x.y, n.x.z, 0.0],
                [n.y.x, n.y.y, n.y.z, 0.0],
                [n.z.x, n.z.y, n.z.z, 0.0],
            ],
        }
    }

    pub fn identity() -> Self {
        Self::mat3_to_std140(Mat3::identity())
    }

    pub fn inverse_transpose_mat4(mat: &Mat4) -> Self {
        let mat3x3 = Mat3::from_cols(mat.x.truncate(), mat.y.truncate(), mat.z.truncate());
        let nm = mat3x3.invert().unwrap_or(Mat3::identity()).transpose();

        Self::mat3_to_std140(nm)
    }
}

///shader: [pbr, hdr, skybox]
#[repr(C, align(16))]
#[derive(Default, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlobalUniform {
    pub ibl_enable: u32,
    pub skybox_enable: u32,
    pub exposure: f32,
    pub ibl_intensity: f32,

    pub entity_id: u64,
    pub tonemap_filter: u32,
    pub debug: u32,

    pub env_rotation: f32,
    pub pad: [u32; 3],
}

///shader: [pbr, blinnphong, light]
#[repr(C, align(16))]
#[derive(Default, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    pub view_proj: [[f32; 4]; 4],

    pub color: [f32; 3],
    pub directional: u32,

    pub position: [f32; 3],
    pub cast_shadow: u32,

    pub entity_id: u64,
    pub pad: [i32; 2],
}

impl LightUniform {
    fn new() -> Self {
        Self {
            view_proj: Mat4::identity().into(),
            color: [1.0, 1.0, 1.0],
            directional: 1,
            position: [0.0, 0.0, -1.0],
            ..Default::default()
        }
    }
}

pub const MAX_LIGHTS: usize = 64;
#[repr(C, align(16))]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightsUniform {
    pub lights: [LightUniform; MAX_LIGHTS],

    pub count: u32,
    pub enabled: u32,
    pub pad: [u32; 2],
}

impl Default for LightsUniform {
    fn default() -> Self {
        Self {
            lights: [LightUniform::new(); MAX_LIGHTS],
            count: 0,
            enabled: 0,
            pad: [0; 2],
        }
    }
}

///shader: [pbr, blinnphong]
#[repr(C, align(16))]
#[derive(Default, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    pub color_factor: [f32; 4],
    pub emissive_factor: [f32; 4],

    pub roughness_factor: f32,
    pub metallic_factor: f32,
    pub normal_scale: f32,
    pub occlusion_strength: f32,

    pub texture_flags: u32,
    pub alpha_mode: u32,
    pub alpha_cutoff: f32,
    pub transmission_factor: f32,

    pub is_trasmissive: u32,
    pub is_volume: u32,
    pub thickness_factor: f32,
    pub attenuation_distance: f32,

    pub attenuation_color: [f32; 3],
    pub ior: f32,

    pub sheen_color_factor: [f32; 3],
    pub sheen_roughness_factor: f32,

    pub texture_transforms: [Mat3Std140; crate::assets::material_desc::MATERIAL_TEXTURE_COUNT],

    pub coord_flags: u32,
    pub is_sheen: u32,

    pub _pad: [u32; 2],
}
