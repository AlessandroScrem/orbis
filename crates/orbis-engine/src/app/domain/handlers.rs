use std::collections::HashSet;

use crate::EntityRawU64;
use crate::app::domain::events::*;
use crate::app::*;
use crate::assets::IblAsset;
use crate::assets::MaterialAsset;
use crate::ecs::components::light;
use crate::ecs::components::*;
use crate::engine::RuntimeEvent;
use crate::engine::core::EventBus;
use crate::prelude::*;
use crate::scene;

impl App {
    pub fn update_domain_event(&mut self, bus: &mut EventBus) {
        let mut domain_events = bus.drain_domain();
        while let Some(event) = domain_events.pop() {
            match event {
                DomainEvent::Camera(event) => handle_camera_event(self, event),
                DomainEvent::Global(event) => handle_global_event(self, event, bus),
                DomainEvent::Entity(event) => handle_entity_event(self, event),
                DomainEvent::Assets(event) => handle_asset_event(self, event, bus),
                DomainEvent::Selection(event) => handle_selection_event(self, event, bus),
                DomainEvent::Scene(event) => handle_scene_event(self, event, bus),
            }
        }
    }
}

pub fn handle_camera_event(app: &mut App, event: CameraEvent) {
    match event {
        CameraEvent::RecenterCamera => app.recenter_camera(),
        CameraEvent::CameraFov(fov) => app.camera.set_fov(fov),
        CameraEvent::CameraDistance(distance) => app.camera.set_distance(distance),
        CameraEvent::CameraNearFar(near_far) => app.camera.set_near_far(near_far),
        CameraEvent::CameraOrbit(dx, dy) => app.camera.orbit((dx, dy)),
        CameraEvent::CameraPan(dx, dy) => app.camera.pan((dx, dy)),
        CameraEvent::CameraZoom(delta) => app.camera.zoom(delta),
    }
}

pub fn handle_scene_event(app: &mut App, event: SceneEvent, bus: &mut EventBus) {
    match event {
        SceneEvent::ClearScene => {
            app.current_scene.clear_scene(&mut app.asset_mgr);
            app.selected.clear();
            app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
        }
        SceneEvent::SaveAs(path) => {
            if app.current_scene.save_scene_json(&path).is_ok() {
                add_recent_file(app, path);
            }
        }
        SceneEvent::Save => {
            if app.current_scene.save().is_ok()
                && let Some(path) = app.current_scene.filename.clone()
            {
                add_recent_file(app, path.into());
            }
        }
        SceneEvent::Open(path) => {
            app.current_scene.clear_scene(&mut app.asset_mgr);
            app.selected.clear();
            if app
                .current_scene
                .open_scene(&path, &mut app.asset_mgr, bus)
                .is_ok()
            {
                add_recent_file(app, path);
            }
        }
        SceneEvent::AddComponent(loaded_scene, transform) => {
            scene::spawn_scene(
                &mut app.current_scene.world,
                &loaded_scene,
                &app.asset_mgr,
                transform,
            );
            app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
            bus.send_domain(DomainEvent::Camera(CameraEvent::RecenterCamera));
        }

        SceneEvent::AddLightComponent(light_component, tag, transform) => {
            light::add(
                &mut app.current_scene.world,
                light_component,
                tag,
                transform,
            );
            app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
        }
    }
}

fn add_recent_file(app: &mut App, path: std::path::PathBuf) {
    app.settings.add_recent_file(path.into());
    let _ = app.settings.save();
    app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
}

pub fn handle_global_event(app: &mut App, event: GlobalEvent, bus: &mut EventBus) {
    let g = &mut app.globals;
    match event {
        GlobalEvent::LightEnable(flag) => {
            g.light_enable = flag;
            bus.send_domain(DomainEvent::Entity(EntityEvent::EnableAllLight(flag)));
        }
        GlobalEvent::IblEnable(flag) => g.ibl_enable = flag,
        GlobalEvent::SkyboxEnable(flag) => g.skybox_enable = flag,
        GlobalEvent::SkyboxEnableBlur(flag) => g.skybox_enable_blur = flag,
        GlobalEvent::AxisEnable(flag) => g.axis_enable = flag,
        GlobalEvent::BboxEnable(flag) => g.bbox_enable = flag,
        GlobalEvent::BboxAxisAligned(flag) => g.bbox_axis_aligned = flag,
        GlobalEvent::DebugCode(code) => g.debug_code = code,
        GlobalEvent::Exposure(value) => g.exposure = value,
        GlobalEvent::IblIntensity(value) => g.ibl_intensity = value,
        GlobalEvent::TonemapFilter(filter_code) => g.tonemap_filter = filter_code,
        GlobalEvent::MipsCsEnable(flag) => g.mips_cp = flag,
        GlobalEvent::EnvRotation(value) => g.env_rotation = value,
    }
}

pub fn handle_entity_event(app: &mut App, event: EntityEvent) {
    let world = &mut app.current_scene.world;
    match event {
        EntityEvent::RemoveEntity(entity) => {
            hierarchy::remove_entity(&mut app.asset_mgr, entity, world);
            app.selected.remove(&entity);
            app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
        }
        EntityEvent::AddParent(entity) => {
            hierarchy::add_parent(entity, world);
            app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
        }
        EntityEvent::EnableAllLight(enable) => light::enable_all_lights(enable, world),
        EntityEvent::AddLight => {
            light::new(world);
            app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
        }
        EntityEvent::DisableEntity(entity, disable) => {
            hierarchy::disable_entity(entity, world, disable);
            app.editor_scene_revision = app.editor_scene_revision.wrapping_add(1);
        }
    }
}

pub fn handle_asset_event(app: &mut App, event: AssetEvent, bus: &mut EventBus) {
    match event {
        AssetEvent::UpdateMaterial(material_id, desc) => {
            app.asset_mgr
                .update::<MaterialAsset>(material_id, |asset| asset.desc = desc);
        }
        AssetEvent::LoadGltf(path) => {
            if let Some(loaded) = crate::assets::gltf_loader::load_gltf(path, &mut app.asset_mgr) {
                info!("Loaded: {} Meshes", loaded.meshes.len());
                bus.send_domain(DomainEvent::Scene(SceneEvent::AddComponent(
                    loaded,
                    TransformComponent::default(),
                )));
            }
        }
        AssetEvent::AddIbl(path) => {
            use crate::assets::texture_asset::*;
            let texture_asset = create_texture(path.clone(), TextureUsage::HDR16);
            let hdr_id = app.asset_mgr.add::<TextureAsset>(texture_asset);
            app.asset_mgr.add::<IblAsset>(IblAsset::new(hdr_id, path));
            app.editor_ibl_revision = app.editor_ibl_revision.wrapping_add(1);
        }
    }
}

pub fn handle_selection_event(app: &mut App, event: SelectionEvent, bus: &mut EventBus) {
    match event {
        SelectionEvent::Hovered(entity) => app.hovered = entity,
        SelectionEvent::Select(entities) => {
            app.selected = entities
                .into_iter()
                .map(EntityRawU64::from_raw_u64)
                .collect::<HashSet<_>>();
        }
        SelectionEvent::SelectIbl(ibl_id) => {
            app.selected_ibl = Some(ibl_id);
            app.editor_ibl_revision = app.editor_ibl_revision.wrapping_add(1);
            bus.send_runtime(RuntimeEvent::UpdateIblMaps(ibl_id));
        }
    }
}
