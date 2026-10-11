use crate::BoundingBox;
use crate::math::*;
use legion::Entity;

use crate::assets::MeshId;

use serde::{Deserialize, Serialize};

// Ecs Components
#[derive(Clone, Serialize, Deserialize)]
pub struct LightComponent {
    pub color: [f32; 3],
    pub directional: bool,
    pub cast_shadow: bool,
    pub frustum: bool,
}
impl Default for LightComponent {
    fn default() -> Self {
        const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

        Self {
            color: WHITE,
            frustum: false,
            cast_shadow: false,
            directional: true,
        }
    }
}

#[derive(Default, Clone)]
pub struct MeshComponent {
    pub handle: MeshId,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TransformComponent {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl TransformComponent {
    pub fn from_gltf(g_node: &gltf::Node<'_>) -> Self {
        let (position, r, scale) = g_node.transform().decomposed();
        let quat = Quat::new(r[3], r[0], r[1], r[2]);
        let euler = Euler::from(quat);
        let rotation = [euler.x.0, euler.y.0, euler.z.0];
        Self {
            position,
            rotation,
            scale,
        }
    }
}

impl Default for TransformComponent {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Clone)]
pub struct GlobalModelComponent {
    pub mat: Mat4,
}

impl Default for GlobalModelComponent {
    fn default() -> Self {
        Self {
            mat: Mat4::identity(),
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct TagComponent {
    pub name: String,
}

#[derive(Clone)]
pub struct BoundingBoxComponent {
    pub global_bounding_box: BoundingBox,
    pub bounding_box: BoundingBox,
}

#[derive(Default, Clone)]
pub struct HierarchyComponent {
    pub parent: Option<Entity>,
    pub children: Vec<Entity>,
}

#[derive(Clone, Debug)]
pub struct SceneComponent {
    /// Asset da cui proviene la scena
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct Hidden;
