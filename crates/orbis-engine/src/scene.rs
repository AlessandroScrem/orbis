use std::{fs, path::Path};

use legion::*;
use serde::{Deserialize, Serialize};

use crate::{
    Globals,
    app::domain::events::{DomainEvent, SceneEvent},
    assets::{
        MeshAsset,
        asset_manager::AssetManager,
        gltf_loader::{GltfGroup, NodeData, load_gltf},
    },
    ecs::components::*,
    engine::{RuntimeEvent, core::EventBus},
    renderer::render_objects::RenderObjects,
};

#[derive(Serialize, Deserialize, Default)]
pub struct SceneFile {
    pub version: u32,
    pub scenes: Vec<SceneEntry>,
    pub lights: Vec<LightEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct SceneEntry {
    pub path: String,
    pub transform: TransformComponent,
}

#[derive(Serialize, Deserialize)]
pub struct LightEntry {
    pub light_component: LightComponent,
    pub tag: TagComponent,
    pub transform: TransformComponent,
}

pub struct Scene {
    pub filename: Option<String>,
    pub world: World,
    pub resources: Resources,
    pub schedule: Schedule,
    dirty: bool,
    pub render_objects: RenderObjects,
}

impl Default for Scene {
    fn default() -> Self {
        let world = World::default();
        let schedule = crate::ecs::create_current_scene_schedule_builder();
        let resources = Resources::default();
        Scene {
            filename: None,
            world,
            resources,
            schedule,
            dirty: true,
            render_objects: RenderObjects::default(),
        }
    }
}

impl Scene {
    pub fn update_scene(&mut self, bus: &mut EventBus, globals: &Globals) {
        self.schedule.execute(&mut self.world, &mut self.resources);
        self.render_objects = RenderObjects::build(&self.world, globals);

        if self.dirty {
            let title = self
                .filename
                .as_deref()
                .and_then(|path| Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled scene *");
            bus.send_runtime(RuntimeEvent::SetWindowTitle(title.into()));
            self.dirty = false;
        }
    }
}

impl Scene {
    pub fn clear_scene(&mut self, asset_mgr: &mut AssetManager) {
        for entity in hierarchy::collect_hierarchy_root_entities(&self.world).iter() {
            hierarchy::remove_entity(asset_mgr, *entity, &mut self.world);
        }
        self.filename = None;
        self.dirty = true;
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Nessun file scena aperto"))?;
        self.save_scene_json(filename)
    }

    pub fn save_scene_json<P: AsRef<Path>>(&mut self, filename: P) -> anyhow::Result<()> {
        let mut scenes = Vec::new();
        let mut query = <(&SceneComponent, &TransformComponent)>::query();
        for (scene, transform) in query.iter(&self.world) {
            scenes.push(SceneEntry {
                path: scene.path.clone(),
                transform: transform.clone(),
            });
        }
        let mut lights = Vec::new();
        let mut query = <(&LightComponent, &TagComponent, &TransformComponent)>::query();
        for (light, tag, transform) in query.iter(&self.world) {
            lights.push(LightEntry {
                light_component: light.clone(),
                tag: tag.clone(),
                transform: transform.clone(),
            });
        }
        let file = SceneFile {
            version: 1,
            scenes,
            lights,
        };
        let json = serde_json::to_string_pretty(&file)?;
        fs::write(&filename, json)?;
        let string_name = filename.as_ref().to_string_lossy().to_string();
        self.filename = Some(string_name);
        self.dirty = true;
        Ok(())
    }

    pub fn open_scene<P: AsRef<Path>>(
        &mut self,
        filename: P,
        asset_mgr: &mut AssetManager,
        bus: &mut EventBus,
    ) -> anyhow::Result<()> {
        let json = fs::read_to_string(&filename)?;
        let scene_file: SceneFile = serde_json::from_str(&json)?;

        for scene in scene_file.scenes {
            let pathname = std::path::PathBuf::from(&scene.path);
            let loaded = load_gltf(pathname, asset_mgr);
            let root_transform = scene.transform.clone();
            if let Some(loaded) = loaded {
                bus.send_domain(DomainEvent::Scene(SceneEvent::AddComponent(
                    loaded,
                    root_transform,
                )));
            }
            println!("Loading scene: {}", scene.path);
        }
        for light in scene_file.lights {
            bus.send_domain(DomainEvent::Scene(SceneEvent::AddLightComponent(
                light.light_component,
                light.tag,
                light.transform,
            )));
            // Add the light component to the world
        }

        self.filename = Some(filename.as_ref().to_string_lossy().to_string());
        self.dirty = true;
        Ok(())
    }
}

fn collect_mesh_nodes(nodes: &[NodeData], index: usize, output: &mut Vec<usize>) -> bool {
    let node = &nodes[index];
    let mut has_mesh_child = false;
    for &child in &node.children {
        if collect_mesh_nodes(nodes, child, output) {
            has_mesh_child = true;
        }
    }
    if node.mesh.is_some() || has_mesh_child {
        output.push(index);
        true
    } else {
        false
    }
}

pub fn spawn_scene(
    world: &mut legion::World,
    loaded: &GltfGroup,
    asset_mgr: &AssetManager,
    root_transform: TransformComponent,
) {
    let mut mesh_node_indices = Vec::new();
    collect_mesh_nodes(&loaded.nodes, 0, &mut mesh_node_indices);
    let mut node_to_entity = vec![None; loaded.nodes.len()];

    for node_idx in mesh_node_indices.iter() {
        let node = &loaded.nodes[*node_idx];
        let transform = if *node_idx == 0 {
            root_transform.clone()
        } else {
            node.local_transform.clone()
        };
        let entity = world.push((
            TagComponent {
                name: node.name.clone(),
            },
            transform,
            HierarchyComponent::default(),
            GlobalModelComponent::default(),
        ));
        if *node_idx == 0
            && let Some(mut entry) = world.entry(entity)
        {
            entry.add_component(SceneComponent {
                path: loaded.name.clone(),
            });
        }
        node_to_entity[*node_idx] = Some(entity);
    }

    for node_idx in mesh_node_indices.iter() {
        let node = &loaded.nodes[*node_idx];
        if let Some(mesh_idx) = node.mesh {
            let entity = node_to_entity[*node_idx].unwrap();
            let mesh_id = &loaded.meshes[mesh_idx];
            if let Some(mut entry) = world.entry(entity) {
                entry.add_component(MeshComponent { handle: *mesh_id });
                if let Some(mesh_asset) = asset_mgr.get::<MeshAsset>(*mesh_id) {
                    let bbox = &mesh_asset.desc.bounds;
                    entry.add_component(BoundingBoxComponent {
                        bounding_box: bbox.clone(),
                        global_bounding_box: bbox.clone(),
                    });
                }
            }
        }
    }

    for node_idx in mesh_node_indices.iter() {
        let node = &loaded.nodes[*node_idx];
        let Some(parent) = node_to_entity[*node_idx] else {
            continue;
        };
        for &child_idx in &node.children {
            let Some(child) = node_to_entity[child_idx] else {
                continue;
            };
            if let Ok(mut entry) = world.entry_mut(parent)
                && let Ok(h) = entry.get_component_mut::<HierarchyComponent>()
            {
                h.children.push(child);
            }
            if let Ok(mut entry) = world.entry_mut(child)
                && let Ok(h) = entry.get_component_mut::<HierarchyComponent>()
            {
                h.parent = Some(parent);
            }
        }
    }
}
