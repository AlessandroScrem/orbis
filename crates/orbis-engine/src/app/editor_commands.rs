use legion::EntityStore;

use crate::EntityRawU64;
use crate::app::domain::events::DomainEvent::Selection;
use crate::app::domain::events::SelectionEvent;
use crate::app::{
    App,
    domain::events::{AssetEvent, CameraEvent, DomainEvent, EntityEvent, GlobalEvent, SceneEvent},
};
use crate::ecs::components::{LightComponent, TagComponent, TransformComponent};
use crate::editor::{
    EditorCommand, EditorEvent, EntityCommand, MaterialCommand, PickCommand, TransformData,
};
use crate::engine::core::EventBus;

impl App {
    pub(crate) fn dispatch_editor_command(
        &mut self,
        command: EditorCommand,
        bus: &mut EventBus,
    ) -> Option<EditorEvent> {
        match command {
            EditorCommand::Selection(command) => {
                self.selection_command(command, bus);
                None
            }
            EditorCommand::Entity(command) => self.entity_command(command, bus),
            EditorCommand::Scene(command) => {
                self.scene_command(command, bus);
                None
            }
            EditorCommand::Asset(command) => {
                self.asset_command(command, bus);
                None
            }
            EditorCommand::Material(command) => {
                self.material_command(command, bus);
                None
            }
            EditorCommand::Camera(command) => {
                self.camera_command(command, bus);
                None
            }
            EditorCommand::Global(command) => {
                self.global_command(command, bus);
                None
            }
            EditorCommand::Pick(PickCommand::Region { origin, size }) => {
                bus.send_runtime(crate::engine::RuntimeEvent::ReadbackSelection(origin, size));
                None
            }
            EditorCommand::Exit => None,
        }
    }

    fn entity_command(
        &mut self,
        command: EntityCommand,
        bus: &mut EventBus,
    ) -> Option<EditorEvent> {
        match command {
            EntityCommand::SetTransform { entity, transform } => {
                if let Ok(mut entry) = self
                    .current_scene
                    .world
                    .entry_mut(EntityRawU64::from_raw_u64(entity))
                    && let Ok(component) = entry.get_component_mut::<TransformComponent>()
                {
                    *component = TransformComponent {
                        position: transform.translation,
                        rotation: transform.rotation,
                        scale: transform.scale,
                    };
                }
                Some(EditorEvent::TransformChanged { entity, transform })
            }
            EntityCommand::SetName { entity, name } => {
                if let Ok(mut entry) = self
                    .current_scene
                    .world
                    .entry_mut(EntityRawU64::from_raw_u64(entity))
                    && let Ok(tag) = entry.get_component_mut::<TagComponent>()
                {
                    tag.name = name.clone();
                }
                Some(EditorEvent::NameChanged { entity, name })
            }
            EntityCommand::SetLight { entity, light } => {
                let raw_entity = EntityRawU64::from_raw_u64(entity);
                if let Ok(mut entry) = self.current_scene.world.entry_mut(raw_entity)
                    && let Ok(component) = entry.get_component_mut::<LightComponent>()
                {
                    component.color = light.color;
                    component.directional = light.directional;
                    component.cast_shadow = light.cast_shadow;
                    component.frustum = light.frustum;
                }
                Some(EditorEvent::LightChanged { entity, light })
            }
            EntityCommand::Remove { entities } => {
                for entity in entities {
                    bus.send_domain(DomainEvent::Entity(EntityEvent::RemoveEntity(
                        EntityRawU64::from_raw_u64(entity),
                    )));
                }
                None
            }
            EntityCommand::BeginTransformEdit { entity } => {
                let entity = EntityRawU64::from_raw_u64(entity);
                if let Some(transform) = self.transform_for(entity) {
                    self.transform_edit = Some((
                        entity,
                        TransformData {
                            translation: transform.position,
                            rotation: transform.rotation,
                            scale: transform.scale,
                        },
                    ));
                }
                None
            }
            EntityCommand::EndTransformEdit { entity } => {
                let entity = EntityRawU64::from_raw_u64(entity);
                self.transform_edit.take().filter(|(id, _)| *id == entity);
                None
            }
            EntityCommand::AddLight => {
                bus.send_domain(DomainEvent::Entity(EntityEvent::AddLight));
                None
            }
            EntityCommand::AddParent { entity } => {
                bus.send_domain(DomainEvent::Entity(EntityEvent::AddParent(
                    EntityRawU64::from_raw_u64(entity),
                )));
                None
            }
            EntityCommand::SetEnabled { entity, enabled } => {
                bus.send_domain(DomainEvent::Entity(EntityEvent::DisableEntity(
                    EntityRawU64::from_raw_u64(entity),
                    !enabled,
                )));
                None
            }
        }
    }

    fn selection_command(&mut self, command: crate::editor::SelectionCommand, bus: &mut EventBus) {
        use crate::editor::SelectionCommand;

        match command {
            SelectionCommand::Select { entities } => {
                bus.send_domain(Selection(SelectionEvent::Select(entities)));
            }
            SelectionCommand::SelectIbl { id } => {
                bus.send_domain(Selection(SelectionEvent::SelectIbl(id)));
            }
        }
    }

    fn scene_command(&mut self, command: crate::editor::SceneCommand, bus: &mut EventBus) {
        use crate::editor::SceneCommand;

        match command {
            SceneCommand::Open(path) => bus.send_domain(DomainEvent::Scene(SceneEvent::Open(path))),
            SceneCommand::Save => bus.send_domain(DomainEvent::Scene(SceneEvent::Save)),
            SceneCommand::SaveAs(path) => {
                bus.send_domain(DomainEvent::Scene(SceneEvent::SaveAs(path)))
            }
            SceneCommand::Clear => bus.send_domain(DomainEvent::Scene(SceneEvent::ClearScene)),
        }
    }

    fn asset_command(&mut self, command: crate::editor::AssetCommand, bus: &mut EventBus) {
        use crate::editor::AssetCommand;

        match command {
            AssetCommand::LoadGltf(path) => {
                bus.send_domain(DomainEvent::Assets(AssetEvent::LoadGltf(path)))
            }
            AssetCommand::AddIbl(path) => {
                bus.send_domain(DomainEvent::Assets(AssetEvent::AddIbl(path)))
            }
        }
    }

    fn material_command(&mut self, command: MaterialCommand, bus: &mut EventBus) {
        match command {
            MaterialCommand::Update { id, desc } => {
                bus.send_domain(DomainEvent::Assets(AssetEvent::UpdateMaterial(id, desc)));
            }
        }
    }

    fn camera_command(&mut self, command: crate::editor::CameraCommand, bus: &mut EventBus) {
        use crate::editor::CameraCommand;

        match command {
            CameraCommand::Recenter => {
                bus.send_domain(DomainEvent::Camera(CameraEvent::RecenterCamera))
            }
            CameraCommand::SetFov(value) => {
                bus.send_domain(DomainEvent::Camera(CameraEvent::CameraFov(value)))
            }
            CameraCommand::SetDistance(value) => {
                bus.send_domain(DomainEvent::Camera(CameraEvent::CameraDistance(value)))
            }
            CameraCommand::SetNearFar { near, far } => bus.send_domain(DomainEvent::Camera(
                CameraEvent::CameraNearFar((near.max(0.1), far.max(near + 0.1))),
            )),
        }
    }

    fn global_command(&mut self, command: crate::editor::GlobalCommand, bus: &mut EventBus) {
        use crate::editor::GlobalCommand;

        match command {
            GlobalCommand::SetLightEnable(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::LightEnable(value)))
            }
            GlobalCommand::SetIblEnable(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::IblEnable(value)))
            }
            GlobalCommand::SetSkyboxEnable(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::SkyboxEnable(value)))
            }
            GlobalCommand::SetSkyboxBlur(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::SkyboxEnableBlur(value)))
            }
            GlobalCommand::SetAxisEnable(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::AxisEnable(value)))
            }
            GlobalCommand::SetBoundingBoxEnable(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::BboxEnable(value)))
            }
            GlobalCommand::SetBoundingBoxAxisAligned(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::BboxAxisAligned(value)))
            }
            GlobalCommand::SetMipsWithCompute(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::MipsCsEnable(value)))
            }
            GlobalCommand::SetEnvironmentRotation(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::EnvRotation(value)))
            }
            GlobalCommand::SetDebugCode(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::DebugCode(value)))
            }
            GlobalCommand::SetExposure(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::Exposure(value)))
            }
            GlobalCommand::SetIblIntensity(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::IblIntensity(value)))
            }
            GlobalCommand::SetTonemap(value) => {
                bus.send_domain(DomainEvent::Global(GlobalEvent::TonemapFilter(value)))
            }
        }
    }
}
