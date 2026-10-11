use crate::EntityRawU64;
use crate::app::App;
use crate::editor::{EditorCommand, EditorEvent, EntityId, Query, QueryResult};
use crate::engine::{editor::EditorBackend, core::EventBus};

impl EditorBackend for App {
    fn query(&self, query: &Query) -> QueryResult {
        match query {
            Query::Hierarchy => QueryResult::Hierarchy(self.hierarchy_data()),
            Query::Entity { entity } => QueryResult::Entity(self.entity_data(*entity)),
            Query::Children { parent } => QueryResult::Children(self.children_data(*parent)),
            Query::Inspector { entity } => QueryResult::Inspector(self.inspector_data(*entity)),
            Query::Selection => QueryResult::Selection(self.editor_selection()),
            Query::Settings => QueryResult::Settings(self.editor_settings()),
            Query::Statistics => QueryResult::Statistics(Default::default()),
            Query::SceneSettings => QueryResult::SceneSettings(self.scene_settings()),
            Query::Ibls => QueryResult::Ibls(self.ibl_data()),
            Query::ResourceStats => QueryResult::ResourceStats(Default::default()),
        }
    }

    fn execute_command(
        &mut self,
        command: EditorCommand,
        bus: &mut EventBus,
    ) -> Option<EditorEvent> {
        match command {
            EditorCommand::Exit => {
                bus.send_runtime(crate::engine::RuntimeEvent::CloseRequested);
                None
            }
            command => {
                let settings_changed = command.settings_changed();
                if settings_changed {
                    self.dispatch_editor_command(command, bus);
                    Some(EditorEvent::SettingsChanged)
                } else {
                    self.dispatch_editor_command(command, bus)
                }
            }
        }
    }

    fn editor_scene_revision(&self) -> u64 {
        self.editor_scene_revision
    }

    fn editor_ibl_revision(&self) -> u64 {
        self.editor_ibl_revision
    }

    fn editor_selection(&self) -> Vec<EntityId> {
        self.selected.iter().map(EntityRawU64::as_raw_u64).collect()
    }
}
