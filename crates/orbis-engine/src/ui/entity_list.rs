use crate::editor::{
    AssetCommand, EditorCommand, EntityCommand, EntityId, HierarchyData, HierarchyNode,
    SelectionCommand,
};
use crate::ui::ui_commands::UiCommands;
use crate::ui::ui_layer::{Layer, UiContext};
use imgui::{Condition, Key, MouseButton, StyleColor, TreeNodeFlags, TreeNodeId, Ui};
use std::collections::HashSet;

const ICON_LIGHTBULB: &str = "\u{EA61}";
const ICON_TRASH: &str = "\u{EA81}";
const ICON_ADD: &str = "\u{EA60}";
const ICON_FOLDER: &str = "\u{EAF7}";
const ICON_CLEAR: &str = "\u{EAC0}";
const ICON_GEAR: &str = "\u{EAF8}";
const ICON_EYE: &str = "\u{EA70}";
const ICON_LAYER: &str = "\u{EBD2}";
const ICON_LAYER_DOT: &str = "\u{EBD3}";
const ICON_LAYER_ACTIVE: &str = "\u{EBD4}";

#[derive(Default)]
pub struct EntityListUi {
    hierarchy: Option<HierarchyData>,
    selection: Vec<EntityId>,
}

impl Layer for EntityListUi {
    fn update(&mut self, commands: &UiCommands) {
        self.hierarchy = commands.hierarchy().cloned();
        self.selection = commands.selection().to_vec();
    }

    fn build(&mut self, ui: &Ui, ctx: &mut UiContext) {
        ui.window("Entities")
            .size([340.0, 420.0], Condition::FirstUseEver)
            .build(|| {
                toolbar(ui, ctx);
                let Some(hierarchy) = self.hierarchy.as_ref() else {
                    ui.text("Loading hierarchy...");
                    return;
                };
                let mut selection: HashSet<EntityId> = self.selection.iter().copied().collect();
                let old_selection = selection.clone();
                let mut action = None;
                ui.text("Meshes");
                ui.separator();
                for node in hierarchy
                    .nodes
                    .iter()
                    .filter(|node| node.parent.is_none() && !node.is_light)
                {
                    draw_node(ui, node, hierarchy, &mut selection, &mut action, ctx);
                }
                ui.separator();
                ui.text("Lights");
                ui.separator();
                for node in hierarchy.nodes.iter().filter(|node| node.is_light) {
                    draw_light_node(ui, node, &mut selection, &mut action, ctx);
                }
                if selection != old_selection {
                    ctx.commands
                        .send(EditorCommand::Selection(SelectionCommand::Select {
                            entities: selection.iter().copied().collect(),
                        }));
                }
                if let Some(command) = action {
                    ctx.commands.send(command);
                }
                if ui.is_window_hovered()
                    && !ui.is_any_item_hovered()
                    && ui.is_mouse_clicked(MouseButton::Left)
                {
                    ctx.commands
                        .send(EditorCommand::Selection(SelectionCommand::Select {
                            entities: Vec::new(),
                        }));
                }
            });
    }
}

fn toolbar(ui: &Ui, ctx: &mut UiContext) {
    if ui.small_button(format!("{ICON_FOLDER}##load_gltf"))
        && let Some(path) = crate::ui::menu_bar::file_open(crate::ui::menu_bar::FileFilter::Gltf)
    {
        ctx.commands
            .send(EditorCommand::Asset(AssetCommand::LoadGltf(path)));
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Load glTF / GLB");
    }
    ui.same_line();
    if ui.small_button(format!("{ICON_LIGHTBULB}##add_light")) {
        ctx.commands
            .send(EditorCommand::Entity(EntityCommand::AddLight));
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Add light");
    }
    ui.same_line();
    if ui.small_button(format!("{ICON_CLEAR}##clear_selection")) {
        ctx.commands
            .send(EditorCommand::Selection(SelectionCommand::Select {
                entities: Vec::new(),
            }));
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Clear selection");
    }
}

fn draw_node(
    ui: &Ui,
    node: &HierarchyNode,
    hierarchy: &HierarchyData,
    selection: &mut HashSet<EntityId>,
    action: &mut Option<EditorCommand>,
    ctx: &mut UiContext,
) {
    let is_selected = selection.contains(&node.entity);
    let children: Vec<&HierarchyNode> = hierarchy
        .nodes
        .iter()
        .filter(|child| child.parent == Some(node.entity) && !child.is_light)
        .collect();
    let icon = if node.parent.is_none() {
        ICON_LAYER_DOT
    } else if is_selected {
        ICON_LAYER_ACTIVE
    } else {
        ICON_LAYER
    };
    let _disabled =
        (!node.visible).then(|| ui.push_style_color(StyleColor::Text, [1.0, 1.0, 1.0, 0.35]));
    let opened = ui
        .tree_node_config(TreeNodeId::Str(node.entity.to_string()))
        .label::<String, String>(format!("{icon} {}", node.name))
        .leaf(children.is_empty())
        .open_on_arrow(true)
        .open_on_double_click(true)
        .selected(is_selected)
        .push();
    handle_selection_click(ui, node.entity, selection);
    if node.parent.is_none() {
        row_icons(ui, node, action, ctx);
    }
    if let Some(_token) = opened {
        for child in children {
            draw_node(ui, child, hierarchy, selection, action, ctx);
        }
    }
    context_menu(ui, node.entity, action);
}

fn draw_light_node(
    ui: &Ui,
    node: &HierarchyNode,
    selection: &mut HashSet<EntityId>,
    action: &mut Option<EditorCommand>,
    ctx: &mut UiContext,
) {
    let is_selected = selection.contains(&node.entity);
    let flags = TreeNodeFlags::LEAF
        | if is_selected {
            TreeNodeFlags::SELECTED
        } else {
            TreeNodeFlags::empty()
        };

    let _disabled =
        (!node.visible).then(|| ui.push_style_color(StyleColor::Text, [1.0, 1.0, 1.0, 0.35]));

    let _opened = ui
        .tree_node_config(format!("{ICON_LIGHTBULB} {}##{}", node.name, node.entity))
        .flags(flags)
        .push();

    handle_selection_click(ui, node.entity, selection);
    row_icons(ui, node, action, ctx);
    context_menu(ui, node.entity, action);
}

fn handle_selection_click(ui: &Ui, entity: EntityId, selection: &mut HashSet<EntityId>) {
    if !ui.is_item_clicked() || ui.is_item_toggled_open() {
        return;
    }
    let ctrl = ui.is_key_down(Key::LeftCtrl) || ui.is_key_down(Key::RightCtrl);
    if ctrl {
        if !selection.remove(&entity) {
            selection.insert(entity);
        }
    } else {
        selection.clear();
        selection.insert(entity);
    }
}

fn row_icons(
    ui: &Ui,
    node: &HierarchyNode,
    action: &mut Option<EditorCommand>,
    _ctx: &mut UiContext,
) {
    right_icons(ui, |ui| {
        if ui.small_button(format!("{ICON_EYE}##eye{}", node.entity)) {
            *action = Some(EditorCommand::Entity(EntityCommand::SetEnabled {
                entity: node.entity,
                enabled: !node.visible,
            }));
        }
        if ui.is_item_hovered() {
            ui.tooltip_text(if node.visible { "Hide" } else { "Show" });
        }
        ui.same_line();
        if ui.small_button(format!("{ICON_ADD}##add{}", node.entity)) {
            if node.is_light {
                *action = Some(EditorCommand::Entity(EntityCommand::AddLight));
            } else if let Some(path) =
                crate::ui::menu_bar::file_open(crate::ui::menu_bar::FileFilter::Gltf)
            {
                *action = Some(EditorCommand::Asset(AssetCommand::LoadGltf(path)));
            }
        }
        if ui.is_item_hovered() {
            ui.tooltip_text(if node.is_light {
                "Add Light"
            } else {
                "Add glTF / GLB"
            });
        }
        ui.same_line();
        if ui.small_button(format!("{ICON_TRASH}##delete{}", node.entity)) {
            *action = Some(EditorCommand::Entity(EntityCommand::Remove {
                entities: vec![node.entity],
            }));
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Delete entity");
        }
        ui.same_line();
        if ui.small_button(format!("{ICON_GEAR}##properties{}", node.entity)) {
            *action = Some(EditorCommand::Selection(SelectionCommand::Select {
                entities: vec![node.entity],
            }));
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Properties");
        }
    });
}

fn context_menu(ui: &Ui, entity: EntityId, action: &mut Option<EditorCommand>) {
    if ui.is_item_hovered() && ui.is_mouse_clicked(MouseButton::Right) {
        ui.open_popup(format!("entity_context##{}", entity));
    }
    if let Some(popup) = ui.begin_popup(format!("entity_context##{}", entity)) {
        if ui.menu_item("Remove") {
            *action = Some(EditorCommand::Entity(EntityCommand::Remove {
                entities: vec![entity],
            }));
        }
        if ui.menu_item("Add Parent") {
            *action = Some(EditorCommand::Entity(EntityCommand::AddParent { entity }));
        }
        popup.end();
    }
}

fn right_icons<F: FnOnce(&Ui)>(ui: &Ui, f: F) {
    let style = ui.clone_style();
    let padding = style.frame_padding[0];
    let width = ui.calc_text_size(ICON_TRASH)[0] + padding * 2.0;
    let total = width * 4.0 + style.item_spacing[0] * 3.0;
    ui.same_line_with_pos(ui.window_content_region_max()[0] - total);
    f(ui);
}
