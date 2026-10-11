use super::ui_commands::UiCommands;
use super::ui_layer::{Layer, UiContext};
use crate::editor::{EntityCommand, EntityId, InspectorData, InspectorSection, LightData};
use crate::ui::{properties_material, properties_transform};
use imgui::{Condition, Image, TreeNodeFlags, Ui};

#[derive(Default)]
pub struct PropertyUi {
    inspector: Option<InspectorData>,
    draft: Option<InspectorData>,
    selection: Vec<EntityId>,
}

impl Layer for PropertyUi {
    fn update(&mut self, commands: &UiCommands) {
        let inspector = commands.inspector().cloned();

        if self.draft.as_ref().is_none_or(|draft| {
            inspector
                .as_ref()
                .is_none_or(|data| data.entity != draft.entity)
        }) {
            self.draft = inspector.clone();
        }

        self.inspector = inspector;
        self.selection = commands.selection().to_vec();
    }

    fn build(&mut self, ui: &Ui, ctx: &mut UiContext) {
        let window = ui
            .window("Properties")
            .size([620.0, 560.0], Condition::FirstUseEver);
        window.build(|| draw_inspector(ui, ctx, self.draft.as_mut(), &self.selection));
    }
}

fn draw_inspector(
    ui: &Ui,
    ctx: &mut UiContext,
    inspector: Option<&mut InspectorData>,
    selection: &[EntityId],
) {
    let Some(inspector) = inspector else {
        ui.text(format!("{} entities selected", selection.len()));
        for entity in selection {
            ui.text(format!("Entity: {}", entity));
        }
        return;
    };

    ui.text(format!("{}  [#{}]", inspector.name, inspector.entity));
    ui.separator();

    draw_inspector_name(ui, ctx, inspector);

    let _disabled = ui.begin_disabled(!inspector.visible);

    let entity = inspector.entity;
    for section in &mut inspector.sections {
        match section {
            InspectorSection::Transform(transform) => {
                properties_transform::draw(ui, ctx, entity, transform);
            }
            InspectorSection::Mesh(mesh) => draw_mesh(ui, mesh),
            InspectorSection::Materials(materials) => properties_material::draw(ui, ctx, materials),
            InspectorSection::BoundingBox(bbox) => draw_bounding_box(ui, bbox),
            InspectorSection::Light(light) => draw_light(ui, ctx, entity, light),
        }
    }
}

fn draw_inspector_name(ui: &Ui, ctx: &UiContext, inspector: &mut InspectorData) {
    if !ui.collapsing_header(
        "Tag",
        TreeNodeFlags::DEFAULT_OPEN | TreeNodeFlags::ALLOW_ITEM_OVERLAP,
    ) {
        return;
    }

    if ui.input_text("Name", &mut inspector.name).build() {
        ctx.commands.send(EntityCommand::SetName {
            entity: inspector.entity,
            name: inspector.name.clone(),
        });
    }
}

fn draw_mesh(ui: &Ui, mesh: &crate::editor::MeshData) {
    if ui.collapsing_header("Mesh", TreeNodeFlags::DEFAULT_OPEN) {
        ui.text(format!("Mesh: {}", mesh.id));
    }
}

fn draw_bounding_box(ui: &Ui, bbox: &crate::editor::BoundingBoxData) {
    if ui.collapsing_header("Bounding Box", TreeNodeFlags::DEFAULT_OPEN) {
        ui.text(format!("Local min: {:?}", bbox.min));
        ui.text(format!("Local max: {:?}", bbox.max));
        ui.separator();
        ui.text(format!("Global min: {:?}", bbox.global_min));
        ui.text(format!("Global max: {:?}", bbox.global_max));
    }
}

fn draw_light(ui: &Ui, ctx: &UiContext, entity: EntityId, light: &mut LightData) {
    if !ui.collapsing_header("Light", TreeNodeFlags::DEFAULT_OPEN) {
        return;
    }

    if draw_light_properties(ui, light) {
        ctx.commands.send(EntityCommand::SetLight {
            entity,
            light: light.clone(),
        });
    }

    if light.cast_shadow
        && let Some(texture) = ctx.textures.shadow_map
    {
        Image::new(texture, [200.0, 200.0]).build(ui);
    }
}

fn draw_light_properties(ui: &Ui, light: &mut LightData) -> bool {
    let mut edited = false;
    edited |= ui.color_edit3("Color", &mut light.color);
    edited |= ui.checkbox("Directional", &mut light.directional);
    edited |= ui.checkbox("Cast Shadow", &mut light.cast_shadow);
    if light.cast_shadow {
        edited |= ui.checkbox("Frustum", &mut light.frustum);
    }
    edited
}
