use legion::{Entity, EntityStore, IntoQuery};

use crate::EntityRawU64;
use crate::app::App;
use crate::ecs::components::{
    BoundingBoxComponent, HierarchyComponent, LightComponent, MeshComponent, TagComponent,
    TransformComponent,
};
use crate::editor::{
    BoundingBoxData, EditorSettingsData, EntityData, EntityId, HierarchyData, HierarchyNode,
    IblData, InspectorData, InspectorSection, LightData, MaterialDto, MeshData, SceneSettingsData,
    TransformData,
};

impl App {
    pub(crate) fn transform_for(&self, entity: Entity) -> Option<TransformComponent> {
        self.current_scene
            .world
            .entry_ref(entity)
            .ok()
            .and_then(|e| e.get_component::<TransformComponent>().ok().cloned())
    }

    pub(crate) fn entity_data(&self, id: EntityId) -> Option<EntityData> {
        let entry = self
            .current_scene
            .world
            .entry_ref(EntityRawU64::from_raw_u64(id))
            .ok()?;
        let name = entry
            .get_component::<TagComponent>()
            .map(|tag| tag.name.clone())
            .unwrap_or_else(|_| "<unnamed>".into());
        Some(EntityData { id, name })
    }

    pub(crate) fn hierarchy_data(&self) -> HierarchyData {
        let mut nodes = Vec::new();
        let mut query = <(Entity, &HierarchyComponent)>::query();
        for (entity, hierarchy) in query.iter(&self.current_scene.world) {
            let entry = self.current_scene.world.entry_ref(*entity).ok();
            let name = self
                .entity_data(entity.as_raw_u64())
                .map(|d| d.name)
                .unwrap_or_else(|| "<unnamed>".into());
            let visible =
                !crate::ecs::components::hierarchy::is_hidden(&self.current_scene.world, *entity);
            let is_light = entry
                .as_ref()
                .map(|e| e.get_component::<LightComponent>().is_ok())
                .unwrap_or(false);
            nodes.push(HierarchyNode {
                entity: entity.as_raw_u64(),
                parent: hierarchy.parent.map(|p| p.as_raw_u64()),
                name,
                visible,
                is_light,
            });
        }

        nodes.sort_by(|a, b| a.name.cmp(&b.name));
        HierarchyData { nodes }
    }

    pub(crate) fn children_data(&self, parent: EntityId) -> Vec<EntityData> {
        let Some(entry) = self
            .current_scene
            .world
            .entry_ref(EntityRawU64::from_raw_u64(parent))
            .ok()
        else {
            return Vec::new();
        };
        let Ok(hierarchy) = entry.get_component::<HierarchyComponent>() else {
            return Vec::new();
        };
        hierarchy
            .children
            .iter()
            .filter_map(|e| self.entity_data(e.as_raw_u64()))
            .collect()
    }

    pub(crate) fn inspector_data(&self, id: EntityId) -> Option<InspectorData> {
        let entry = self
            .current_scene
            .world
            .entry_ref(EntityRawU64::from_raw_u64(id))
            .ok()?;
        let name = entry
            .get_component::<TagComponent>()
            .map(|tag| tag.name.clone())
            .unwrap_or_else(|_| "<unnamed>".into());
        let transform = entry
            .get_component::<TransformComponent>()
            .map(|t| TransformData {
                translation: t.position,
                rotation: t.rotation,
                scale: t.scale,
            })
            .unwrap_or(TransformData {
                translation: [0.0; 3],
                rotation: [0.0; 3],
                scale: [1.0; 3],
            });
        let mesh = entry
            .get_component::<MeshComponent>()
            .ok()
            .map(|mesh| MeshData {
                id: format!("{:?}", mesh.handle),
            });

        let materials = entry
            .get_component::<MeshComponent>()
            .ok()
            .and_then(|mesh| self.asset_mgr.get::<crate::assets::MeshAsset>(mesh.handle))
            .map(|mesh| {
                let mut ids = std::collections::HashSet::new();
                mesh.desc
                    .submeshes
                    .iter()
                    .filter_map(|submesh| {
                        if ids.insert(submesh.material) {
                            self.asset_mgr
                                .get::<crate::assets::MaterialAsset>(submesh.material)
                                .map(|material| MaterialDto {
                                    id: submesh.material,
                                    name: material.desc.name.clone(),
                                    desc: material.desc.clone(),
                                })
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|materials: &Vec<_>| !materials.is_empty());
        let bounding_box = entry
            .get_component::<BoundingBoxComponent>()
            .ok()
            .map(|bbox| BoundingBoxData {
                min: bbox.bounding_box.min,
                max: bbox.bounding_box.max,
                global_min: bbox.global_bounding_box.min,
                global_max: bbox.global_bounding_box.max,
            });

        let light = entry
            .get_component::<LightComponent>()
            .ok()
            .map(|light| LightData {
                color: light.color,
                directional: light.directional,
                cast_shadow: light.cast_shadow,
                frustum: light.frustum,
            });
        let mut sections = vec![InspectorSection::Transform(transform)];

        if let Some(mesh) = mesh {
            sections.push(InspectorSection::Mesh(mesh));
        }

        if let Some(materials) = materials {
            sections.push(InspectorSection::Materials(materials));
        }

        if let Some(bounding_box) = bounding_box {
            sections.push(InspectorSection::BoundingBox(bounding_box));
        }

        if let Some(light) = light {
            sections.push(InspectorSection::Light(light));
        }

        let visible = !crate::ecs::components::hierarchy::is_hidden(
            &self.current_scene.world,
            EntityRawU64::from_raw_u64(id),
        );

        Some(InspectorData {
            entity: id,
            name,
            visible,
            sections,
        })
    }

    pub(crate) fn ibl_data(&self) -> Vec<IblData> {
        self.asset_mgr
            .iter::<crate::assets::IblAsset>()
            .map(|(id, asset)| {
                let name = asset
                    .path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("IBL")
                    .to_owned();
                IblData {
                    id,
                    texture: asset.hrd_id,
                    name,
                    selected: self.selected_ibl == Some(id),
                }
            })
            .collect()
    }

    pub(crate) fn editor_settings(&self) -> EditorSettingsData {
        let (near, far) = self.camera.get_near_far();
        EditorSettingsData {
            light_enable: self.globals.light_enable,
            ibl_enable: self.globals.ibl_enable,
            skybox_enable: self.globals.skybox_enable,
            skybox_enable_blur: self.globals.skybox_enable_blur,
            axis_enable: self.globals.axis_enable,
            bbox_enable: self.globals.bbox_enable,
            bbox_axis_aligned: self.globals.bbox_axis_aligned,
            mips_cp: self.globals.mips_cp,
            env_rotation: self.globals.env_rotation,
            debug_code: self.globals.debug_code,
            exposure: self.globals.exposure,
            ibl_intensity: self.globals.ibl_intensity,
            tonemap_filter: self.globals.tonemap_filter,
            camera_fov: self.camera.get_fov(),
            camera_distance: self.camera.get_distance(),
            camera_near: near,
            camera_far: far,
        }
    }

    pub(crate) fn scene_settings(&self) -> SceneSettingsData {
        let recent = self
            .settings
            .recent_files
            .iter()
            .map(|i| (i.name.clone(), i.path.clone()))
            .collect();
        SceneSettingsData { recent }
    }
}
