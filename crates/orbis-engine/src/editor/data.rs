pub type EntityId = u64;

#[derive(Clone, Debug)]
pub struct EntityData {
    #[allow(dead_code)]
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct HierarchyData {
    pub nodes: Vec<HierarchyNode>,
}

#[derive(Clone, Debug)]
pub struct HierarchyNode {
    pub entity: EntityId,
    pub parent: Option<EntityId>,
    pub name: String,
    pub visible: bool,
    pub is_light: bool,
}

#[derive(Clone, Debug)]
pub struct InspectorData {
    pub entity: EntityId,
    pub name: String,
    pub visible: bool,
    pub sections: Vec<InspectorSection>,
}

#[derive(Clone, Debug)]
pub struct MaterialDto {
    pub id: crate::assets::MaterialId,
    pub name: String,
    pub desc: crate::assets::material_desc::MaterialDesc,
}

#[derive(Clone, Debug)]
pub enum InspectorSection {
    Transform(TransformData),
    Mesh(MeshData),
    Materials(Vec<MaterialDto>),
    BoundingBox(BoundingBoxData),
    Light(LightData),
}

#[derive(Clone, Debug)]
pub struct MeshData {
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct BoundingBoxData {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub global_min: [f32; 3],
    pub global_max: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct LightData {
    pub color: [f32; 3],
    pub directional: bool,
    pub cast_shadow: bool,
    pub frustum: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransformData {
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct IblData {
    pub id: crate::assets::IblId,
    pub texture: crate::assets::TextureId,
    #[allow(dead_code)]
    pub name: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorStatisticsData {
    pub fps: f32,
    pub frametime: f32,
    pub opaque_draw_calls: u32,
    pub opaque_instances: u32,
    pub transmission_draw_calls: u32,
    pub transmission_instances: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceStatsData {
    pub count: usize,
    pub estimated_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorResourceStatsData {
    pub textures: ResourceStatsData,
    pub materials: ResourceStatsData,
    pub meshes: ResourceStatsData,
    pub ibl: ResourceStatsData,
    pub gpu_textures: ResourceStatsData,
    pub gpu_materials: ResourceStatsData,
    pub gpu_meshes: ResourceStatsData,
    pub gpu_shadows: ResourceStatsData,
    pub gpu_ibl: ResourceStatsData,
}

#[derive(Clone, Debug, Default)]
pub struct SceneSettingsData {
    pub recent: Vec<(String, String)>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct EditorSettingsData {
    pub light_enable: bool,
    pub ibl_enable: bool,
    pub skybox_enable: bool,
    pub skybox_enable_blur: bool,
    pub axis_enable: bool,
    pub bbox_enable: bool,
    pub bbox_axis_aligned: bool,
    pub mips_cp: bool,
    pub env_rotation: f32,
    pub debug_code: u32,
    pub exposure: f32,
    pub ibl_intensity: f32,
    pub tonemap_filter: u32,
    pub camera_fov: f32,
    pub camera_distance: f32,
    pub camera_near: f32,
    pub camera_far: f32,
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub enum EditorEvent {
    TransformChanged {
        entity: EntityId,
        transform: TransformData,
    },
    NameChanged {
        entity: EntityId,
        name: String,
    },
    LightChanged {
        entity: EntityId,
        light: LightData,
    },
    SelectionChanged {
        entities: Vec<EntityId>,
    },
    SceneChanged,
    SettingsChanged,
    StatisticsChanged,
    ResourceStatsChanged,
    IblsChanged,
}
