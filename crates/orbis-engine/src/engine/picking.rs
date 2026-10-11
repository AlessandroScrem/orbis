use crate::app::domain::events::DomainEvent::Selection;
use crate::app::domain::events::SelectionEvent::Hovered;
use crate::ecs::entity_id::EntityRawU64;
use crate::editor::{EditorCommand, SelectionCommand};
use crate::engine::editor::EditorService;
use crate::engine::core::EventBus;
use crate::engine::readback::{QueryResult, ReadbackManager};
use crate::gpu::context::GpuContextRef;
use crate::input::Input;
use legion::Entity;

/// Translates picking results into app/editor events and routes picking requests.
#[derive(Default)]
pub(crate) struct PickingService;

impl PickingService {
    pub(crate) fn handle_input(
        &mut self,
        readback: &mut ReadbackManager,
        input: &Input,
        gpu: &GpuContextRef,
        entity_id_texture: &wgpu::Texture,
        bus: &mut EventBus,
        editor_service: &EditorService,
    ) {
        if let Some(result) = readback.poll_results() {
            match result {
                QueryResult::Pick(id) => {
                    bus.send_domain(Selection(Hovered(id.map(Entity::from_raw_u64))));
                }
                QueryResult::Selection(entities) => {
                    editor_service.send_command(EditorCommand::Selection(
                        SelectionCommand::Select { entities },
                    ));
                }
            }
        }

        if input.is_cursor_moved() {
            Self::request_pick(
                readback,
                gpu,
                entity_id_texture,
                (input.mouse_position.x as u32, input.mouse_position.y as u32),
            );
        }
    }

    pub(crate) fn request_pick(
        readback: &mut ReadbackManager,
        gpu: &GpuContextRef,
        entity_id_texture: &wgpu::Texture,
        position: (u32, u32),
    ) {
        readback.request_pick(gpu, entity_id_texture, position);
    }

    pub(crate) fn request_selection(
        readback: &mut ReadbackManager,
        gpu: &GpuContextRef,
        entity_id_texture: &wgpu::Texture,
        origin: (u32, u32),
        size: (u32, u32),
    ) {
        readback.request_selection(gpu, entity_id_texture, origin, size);
    }
}
