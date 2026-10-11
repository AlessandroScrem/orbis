use crate::editor::{
    EditorCommandClient, EditorConnection, EditorEvent, EditorResourceStatsData,
    EditorSettingsData, EditorStatisticsData, EntityId, HierarchyData, IblData, InspectorData,
    InspectorSection, LightData, Query, QueryId, QueryResponse, QueryResult, SceneSettingsData,
    TransformData,
};
use std::collections::HashMap;

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
enum QuerySlot {
    Hierarchy,
    Selection,
    Inspector,
    Settings,
    Statistics,
    ResourceStats,
    SceneSettings,
    Ibls,
}

pub(crate) struct UiCommands {
    connection: EditorConnection,
    hierarchy: Option<HierarchyData>,
    selection: Vec<EntityId>,
    inspector: Option<InspectorData>,
    settings: Option<EditorSettingsData>,
    statistics: Option<EditorStatisticsData>,
    resource_stats: Option<EditorResourceStatsData>,
    scene_settings: SceneSettingsData,
    ibls: Vec<IblData>,
    pending_queries: HashMap<QueryId, QuerySlot>,
}

impl UiCommands {
    pub(crate) fn new(connection: EditorConnection) -> Self {
        Self {
            connection,
            hierarchy: None,
            selection: Vec::new(),
            inspector: None,
            settings: None,
            statistics: None,
            resource_stats: None,
            scene_settings: SceneSettingsData::default(),
            ibls: Vec::new(),
            pending_queries: HashMap::new(),
        }
    }

    pub(crate) fn command_client(&self) -> &EditorCommandClient {
        &self.connection.commands
    }

    pub(crate) fn hierarchy(&self) -> Option<&HierarchyData> {
        self.hierarchy.as_ref()
    }

    pub(crate) fn selection(&self) -> &[EntityId] {
        &self.selection
    }

    pub(crate) fn inspector(&self) -> Option<&InspectorData> {
        self.inspector.as_ref()
    }

    pub(crate) fn settings(&self) -> Option<&EditorSettingsData> {
        self.settings.as_ref()
    }

    pub(crate) fn statistics(&self) -> Option<&EditorStatisticsData> {
        self.statistics.as_ref()
    }

    pub(crate) fn resource_stats(&self) -> Option<&EditorResourceStatsData> {
        self.resource_stats.as_ref()
    }

    pub(crate) fn scene_settings(&self) -> &SceneSettingsData {
        &self.scene_settings
    }

    pub(crate) fn ibls(&self) -> &[IblData] {
        &self.ibls
    }

    pub(crate) fn process(&mut self) {
        self.process_responses();
        self.process_events();
        self.ensure_queries();
    }

    fn request(&mut self, slot: QuerySlot, query: Query) {
        self.remove_pending(slot);
        let id = self.connection.queries.request(query);
        self.pending_queries.insert(id, slot);
    }

    fn remove_pending(&mut self, slot: QuerySlot) {
        self.pending_queries.retain(|_, pending| *pending != slot);
    }

    fn request_initial_queries(&mut self) {
        self.request(QuerySlot::Hierarchy, Query::Hierarchy);
        self.request(QuerySlot::Selection, Query::Selection);
        self.request(QuerySlot::Settings, Query::Settings);
        self.request(QuerySlot::Statistics, Query::Statistics);
        self.request(QuerySlot::ResourceStats, Query::ResourceStats);
        self.request(QuerySlot::SceneSettings, Query::SceneSettings);
        self.request(QuerySlot::Ibls, Query::Ibls);
    }

    fn invalidate_all(&mut self) {
        self.hierarchy = None;
        self.settings = None;
        self.statistics = None;
        self.resource_stats = None;
        self.scene_settings = SceneSettingsData::default();
        self.ibls.clear();
        self.pending_queries.clear();

        self.inspector = None;

        self.request_initial_queries();
        self.request_inspector();
    }

    fn request_inspector(&mut self) {
        if let [entity] = *self.selection.as_slice() {
            self.request(QuerySlot::Inspector, Query::Inspector { entity });
        } else {
            self.inspector = None;
            self.remove_pending(QuerySlot::Inspector);
        }
    }

    fn apply_query_response(&mut self, response: QueryResponse) {
        let Some(slot) = self.pending_queries.remove(&response.id) else {
            return;
        };

        match (slot, response.result) {
            (QuerySlot::Hierarchy, QueryResult::Hierarchy(data)) => self.hierarchy = Some(data),
            (QuerySlot::Settings, QueryResult::Settings(data)) => {
                self.settings = Some(data);
                self.request_inspector();
            }
            (QuerySlot::Statistics, QueryResult::Statistics(data)) => {
                self.statistics = Some(data);
            }
            (QuerySlot::ResourceStats, QueryResult::ResourceStats(data)) => {
                self.resource_stats = Some(data);
            }
            (QuerySlot::SceneSettings, QueryResult::SceneSettings(data)) => {
                self.scene_settings = data;
            }
            (QuerySlot::Ibls, QueryResult::Ibls(data)) => {
                self.ibls = data;
            }
            (QuerySlot::Selection, QueryResult::Selection(selection)) => {
                self.selection = selection;
                self.request_inspector();
            }
            (QuerySlot::Inspector, QueryResult::Inspector(data)) => {
                self.inspector = data;
            }
            _ => {}
        }
    }

    fn apply_transform_changed(&mut self, entity: EntityId, transform: TransformData) {
        if let Some(inspector) = &mut self.inspector
            && inspector.entity == entity
        {
            for section in &mut inspector.sections {
                if let InspectorSection::Transform(current) = section {
                    *current = transform.clone();
                    break;
                }
            }
        }
    }

    fn apply_name_changed(&mut self, entity: EntityId, name: String) {
        self.request(QuerySlot::Hierarchy, Query::Hierarchy);

        if let Some(inspector) = &mut self.inspector
            && inspector.entity == entity
        {
            inspector.name = name.clone();
        }
    }

    fn apply_light_changed(&mut self, entity: EntityId, light: LightData) {
        if let Some(inspector) = &mut self.inspector
            && inspector.entity == entity
        {
            for section in &mut inspector.sections {
                if let InspectorSection::Light(current) = section {
                    *current = light.clone();
                    break;
                }
            }
        }
    }

    fn apply_event(&mut self, event: EditorEvent) {
        match event {
            EditorEvent::SceneChanged => self.invalidate_all(),
            EditorEvent::SelectionChanged { entities } => {
                self.selection = entities;
                self.request_inspector();
            }
            EditorEvent::TransformChanged { entity, transform } => {
                self.apply_transform_changed(entity, transform);
            }
            EditorEvent::NameChanged { entity, name } => {
                self.apply_name_changed(entity, name);
            }
            EditorEvent::LightChanged { entity, light } => {
                self.apply_light_changed(entity, light);
            }
            EditorEvent::SettingsChanged => {
                self.request(QuerySlot::Settings, Query::Settings);
            }
            EditorEvent::StatisticsChanged => {
                self.request(QuerySlot::Statistics, Query::Statistics);
            }
            EditorEvent::ResourceStatsChanged => {
                self.request(QuerySlot::ResourceStats, Query::ResourceStats);
            }
            EditorEvent::IblsChanged => {
                self.request(QuerySlot::Ibls, Query::Ibls);
            }
        }
    }

    fn process_responses(&mut self) {
        while let Some(response) = self.connection.try_recv_response() {
            self.apply_query_response(response);
        }
    }

    fn process_events(&mut self) {
        while let Some(event) = self.connection.events.try_recv() {
            self.apply_event(event);
        }
    }

    fn ensure_queries(&mut self) {
        self.ensure_hierarchy();
        self.ensure_settings();
        self.ensure_statistics();
        self.ensure_resource_stats();
        self.ensure_scene_settings();
        self.ensure_ibls();
    }

    fn ensure_hierarchy(&mut self) {
        if self.hierarchy.is_none() && !self.has_pending(QuerySlot::Hierarchy) {
            self.request(QuerySlot::Hierarchy, Query::Hierarchy);
        }
    }

    fn ensure_settings(&mut self) {
        if self.settings.is_none() && !self.has_pending(QuerySlot::Settings) {
            self.request(QuerySlot::Settings, Query::Settings);
        }
    }

    fn ensure_statistics(&mut self) {
        if self.statistics.is_none() && !self.has_pending(QuerySlot::Statistics) {
            self.request(QuerySlot::Statistics, Query::Statistics);
        }
    }

    fn ensure_resource_stats(&mut self) {
        if self.resource_stats.is_none() && !self.has_pending(QuerySlot::ResourceStats) {
            self.request(QuerySlot::ResourceStats, Query::ResourceStats);
        }
    }

    fn ensure_scene_settings(&mut self) {
        if self.scene_settings.recent.is_empty() && !self.has_pending(QuerySlot::SceneSettings) {
            self.request(QuerySlot::SceneSettings, Query::SceneSettings);
        }
    }

    fn ensure_ibls(&mut self) {
        if self.ibls.is_empty() && !self.has_pending(QuerySlot::Ibls) {
            self.request(QuerySlot::Ibls, Query::Ibls);
        }
    }

    fn has_pending(&self, slot: QuerySlot) -> bool {
        self.pending_queries
            .values()
            .any(|pending| *pending == slot)
    }
}

#[cfg(test)]
mod tests {
    use super::{QuerySlot, UiCommands};
    use crate::editor::{
        EditorConnection, EditorEvent, EditorSettingsData, EntityId, HierarchyData, InspectorData,
        InspectorSection, LightData, Query, QueryResponse, QueryResult, SceneSettingsData,
        TransformData,
    };

    fn inspector(entity: EntityId, name: &str) -> InspectorData {
        InspectorData {
            entity,
            name: name.to_string(),
            visible: true,
            sections: vec![
                InspectorSection::Transform(TransformData {
                    translation: [0.0; 3],
                    rotation: [0.0; 3],
                    scale: [1.0; 3],
                }),
                InspectorSection::Light(LightData {
                    color: [1.0, 1.0, 1.0],
                    directional: false,
                    cast_shadow: false,
                    frustum: false,
                }),
            ],
        }
    }

    #[test]
    fn stale_inspector_response_is_ignored() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        commands.apply_event(EditorEvent::SelectionChanged { entities: vec![1] });
        let first = service.query_rx.recv().expect("first inspector query");
        assert!(matches!(first.query, Query::Inspector { entity: 1 }));

        commands.apply_event(EditorEvent::SelectionChanged { entities: vec![2] });
        let second = service.query_rx.recv().expect("second inspector query");
        assert!(matches!(second.query, Query::Inspector { entity: 2 }));

        service
            .response_tx
            .send(QueryResponse {
                id: first.id,
                result: QueryResult::Inspector(Some(inspector(1, "stale"))),
            })
            .unwrap();
        service
            .response_tx
            .send(QueryResponse {
                id: second.id,
                result: QueryResult::Inspector(Some(inspector(2, "current"))),
            })
            .unwrap();

        commands.process_responses();

        let current = commands.inspector().expect("current inspector");
        assert_eq!(current.entity, 2);
        assert_eq!(current.name, "current");
    }

    #[test]
    fn selection_with_multiple_entities_clears_inspector_request() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        commands.apply_event(EditorEvent::SelectionChanged {
            entities: vec![1, 2],
        });

        assert!(service.query_rx.try_recv().is_err());
        assert!(commands.inspector().is_none());
    }

    #[test]
    fn inspector_change_events_update_cached_sections() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        commands.apply_event(EditorEvent::SelectionChanged { entities: vec![7] });
        let request = service.query_rx.recv().expect("inspector query");

        service
            .response_tx
            .send(QueryResponse {
                id: request.id,
                result: QueryResult::Inspector(Some(inspector(7, "entity"))),
            })
            .unwrap();
        commands.process_responses();

        let transform = TransformData {
            translation: [1.0, 2.0, 3.0],
            rotation: [4.0, 5.0, 6.0],
            scale: [2.0, 2.0, 2.0],
        };
        commands.apply_event(EditorEvent::TransformChanged {
            entity: 7,
            transform: transform.clone(),
        });
        commands.apply_event(EditorEvent::NameChanged {
            entity: 7,
            name: "renamed".to_string(),
        });
        commands.apply_event(EditorEvent::LightChanged {
            entity: 7,
            light: LightData {
                color: [0.2, 0.4, 0.6],
                directional: true,
                cast_shadow: true,
                frustum: true,
            },
        });

        let current = commands.inspector().expect("cached inspector");
        assert_eq!(current.name, "renamed");

        match &current.sections[0] {
            InspectorSection::Transform(value) => assert_eq!(value, &transform),
            _ => panic!("expected transform section"),
        }

        match &current.sections[1] {
            InspectorSection::Light(value) => {
                assert_eq!(value.color, [0.2, 0.4, 0.6]);
                assert!(value.directional);
                assert!(value.cast_shadow);
                assert!(value.frustum);
            }
            _ => panic!("expected light section"),
        }

        let hierarchy_query = service.query_rx.recv().expect("hierarchy refresh");
        assert!(matches!(hierarchy_query.query, Query::Hierarchy));
    }

    #[test]
    fn stale_inspector_response_is_ignored_when_no_current_response_arrives() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        commands.apply_event(EditorEvent::SelectionChanged { entities: vec![1] });
        let request = service.query_rx.recv().expect("inspector query");

        commands.apply_event(EditorEvent::SelectionChanged { entities: vec![2] });
        assert!(commands.inspector().is_none());

        service
            .response_tx
            .send(QueryResponse {
                id: request.id,
                result: QueryResult::Inspector(Some(inspector(1, "stale"))),
            })
            .unwrap();

        commands.process_responses();

        assert!(commands.inspector().is_none());
        assert_eq!(commands.selection(), &[2]);
    }

    #[test]
    fn unknown_query_response_is_ignored() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        service
            .response_tx
            .send(QueryResponse {
                id: 999,
                result: QueryResult::Selection(vec![42]),
            })
            .unwrap();

        commands.process_responses();

        assert!(commands.selection().is_empty());
    }

    #[test]
    fn settings_and_statistics_events_refresh_only_their_cache() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        commands.apply_event(EditorEvent::SettingsChanged);
        let settings = service.query_rx.recv().expect("settings query");
        assert!(matches!(settings.query, Query::Settings));
        assert!(service.query_rx.try_recv().is_err());

        commands.apply_event(EditorEvent::StatisticsChanged);
        let statistics = service.query_rx.recv().expect("statistics query");
        assert!(matches!(statistics.query, Query::Statistics));
        assert!(service.query_rx.try_recv().is_err());
    }

    #[test]
    fn query_ids_are_unique_and_preserved() {
        let (connection, service) = EditorConnection::new();
        let first = connection.queries.request(Query::Hierarchy);
        let second = connection.queries.request(Query::Selection);

        assert_ne!(first, second);

        let first_request = service.query_rx.recv().expect("first query");
        let second_request = service.query_rx.recv().expect("second query");
        assert_eq!(first_request.id, first);
        assert_eq!(second_request.id, second);

        let mut commands = UiCommands::new(connection);
        commands.pending_queries.insert(first, QuerySlot::Hierarchy);
        commands
            .pending_queries
            .insert(second, QuerySlot::Selection);

        service
            .response_tx
            .send(QueryResponse {
                id: second,
                result: QueryResult::Selection(vec![42]),
            })
            .unwrap();
        commands.process_responses();

        assert_eq!(commands.selection(), &[42]);
        assert!(commands.pending_queries.contains_key(&first));
        assert!(!commands.pending_queries.contains_key(&second));
    }

    #[test]
    fn initial_process_requests_scene_settings() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        commands.process();

        let mut found = false;
        while let Ok(request) = service.query_rx.try_recv() {
            if matches!(request.query, Query::SceneSettings) {
                found = true;
                break;
            }
        }
        assert!(found);
    }

    #[test]
    fn scene_changed_invalidates_cached_editor_data() {
        let (connection, service) = EditorConnection::new();
        let mut commands = UiCommands::new(connection);

        commands.hierarchy = Some(HierarchyData::default());
        commands.settings = Some(EditorSettingsData {
            light_enable: true,
            ibl_enable: true,
            skybox_enable: true,
            skybox_enable_blur: true,
            axis_enable: true,
            bbox_enable: true,
            bbox_axis_aligned: true,
            mips_cp: true,
            env_rotation: 0.0,
            debug_code: 0,
            exposure: 0.0,
            ibl_intensity: 1.0,
            tonemap_filter: 0,
            camera_fov: 1.0,
            camera_distance: 1.0,
            camera_near: 0.1,
            camera_far: 100.0,
        });
        commands.inspector = Some(inspector(7, "entity"));
        commands.scene_settings = SceneSettingsData {
            recent: vec![("scene.json".into(), "/tmp/scene.json".into())],
        };

        commands.apply_event(EditorEvent::SceneChanged);

        assert!(commands.hierarchy().is_none());
        assert!(commands.settings().is_none());
        assert!(commands.inspector().is_none());
        assert!(commands.scene_settings().recent.is_empty());

        for _ in 0..7 {
            let _ = service.query_rx.recv().expect("refresh query");
        }
    }
}
