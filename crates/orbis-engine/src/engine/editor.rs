use crate::editor::{
    EditorCommand, EditorEvent, EditorResourceStatsData, EditorServiceChannels,
    EditorStatisticsData, EntityId, Query, QueryResponse, QueryResult,
};
use crate::engine::core::EventBus;

pub trait EditorBackend {
    fn query(&self, query: &Query) -> QueryResult;
    fn execute_command(
        &mut self,
        command: EditorCommand,
        bus: &mut EventBus,
    ) -> Option<EditorEvent>;
    fn editor_scene_revision(&self) -> u64;
    fn editor_ibl_revision(&self) -> u64;
    fn editor_selection(&self) -> Vec<EntityId>;
}

pub struct EditorService {
    channels: EditorServiceChannels,
    last_scene_revision: u64,
    last_selection: Vec<EntityId>,
    last_ibl_revision: u64,
    statistics: EditorStatisticsData,
    resource_stats: EditorResourceStatsData,
}

impl EditorService {
    pub fn new(channels: EditorServiceChannels) -> Self {
        Self {
            channels,
            last_scene_revision: 0,
            last_selection: Vec::new(),
            last_ibl_revision: 0,
            statistics: EditorStatisticsData::default(),
            resource_stats: EditorResourceStatsData::default(),
        }
    }

    pub fn send_command(&self, command: EditorCommand) {
        let _ = self.channels.command_tx.send(command);
    }

    pub fn set_statistics(&mut self, statistics: EditorStatisticsData) {
        if self.statistics != statistics {
            self.statistics = statistics;
            let _ = self.channels.event_tx.send(EditorEvent::StatisticsChanged);
        }
    }

    pub fn set_resource_stats(&mut self, stats: EditorResourceStatsData) {
        if self.resource_stats != stats {
            self.resource_stats = stats;
            let _ = self
                .channels
                .event_tx
                .send(EditorEvent::ResourceStatsChanged);
        }
    }

    pub fn process<B: EditorBackend>(&mut self, backend: &mut B, bus: &mut EventBus) {
        self.process_commands(backend, bus);
        self.process_queries(backend);
        self.publish_state_changes(backend);
    }

    fn process_commands<B: EditorBackend>(&self, backend: &mut B, bus: &mut EventBus) {
        while let Ok(command) = self.channels.command_rx.try_recv() {
            if let Some(event) = backend.execute_command(command, bus) {
                let _ = self.channels.event_tx.send(event);
            }
        }
    }

    fn process_queries<B: EditorBackend>(&self, backend: &B) {
        while let Ok(request) = self.channels.query_rx.try_recv() {
            let result = match &request.query {
                Query::Statistics => QueryResult::Statistics(self.statistics.clone()),
                Query::ResourceStats => QueryResult::ResourceStats(self.resource_stats.clone()),
                _ => backend.query(&request.query),
            };
            let _ = self.channels.response_tx.send(QueryResponse {
                id: request.id,
                result,
            });
        }
    }

    fn publish_state_changes<B: EditorBackend>(&mut self, backend: &B) {
        let scene_revision = backend.editor_scene_revision();
        if scene_revision != self.last_scene_revision {
            self.last_scene_revision = scene_revision;
            let _ = self.channels.event_tx.send(EditorEvent::SceneChanged);
        }
        let ibl_revision = backend.editor_ibl_revision();
        if ibl_revision != self.last_ibl_revision {
            self.last_ibl_revision = ibl_revision;
            let _ = self.channels.event_tx.send(EditorEvent::IblsChanged);
        }

        let selection = backend.editor_selection();
        if selection != self.last_selection {
            self.last_selection = selection.clone();
            let _ = self.channels.event_tx.send(EditorEvent::SelectionChanged {
                entities: selection,
            });
        }
    }
}
