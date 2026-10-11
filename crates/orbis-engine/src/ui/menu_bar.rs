use super::ui_layer::{Layer, UiContext};
use crate::editor::{AssetCommand, EditorCommand, SceneCommand, SceneSettingsData};
use imgui::Ui;
use std::path::PathBuf;

#[derive(Default)]
pub struct MenuBarUi {
    scene_settings: SceneSettingsData,
}

impl Layer for MenuBarUi {
    fn update(&mut self, commands: &super::ui_commands::UiCommands) {
        self.scene_settings = commands.scene_settings().clone();
    }

    fn build(&mut self, ui: &Ui, ctx: &mut UiContext) {
        if let Some(_bar) = ui.begin_main_menu_bar() {
            if let Some(_menu) = ui.begin_menu("File") {
                if ui.menu_item("New") {
                    ctx.commands.send(EditorCommand::Scene(SceneCommand::Clear));
                }
                if ui.menu_item("Open Scene")
                    && let Some(path) = file_open(FileFilter::Json)
                {
                    ctx.commands
                        .send(EditorCommand::Scene(SceneCommand::Open(path)));
                }
                if ui.menu_item("Save As..")
                    && let Some(path) = file_save(FileFilter::Json)
                {
                    ctx.commands
                        .send(EditorCommand::Scene(SceneCommand::SaveAs(path)));
                }
                if ui.menu_item("Save") {
                    ctx.commands.send(EditorCommand::Scene(SceneCommand::Save));
                }
                ui.separator();
                if ui.menu_item("Load Gltf")
                    && let Some(path) = file_open(FileFilter::Gltf)
                {
                    ctx.commands
                        .send(EditorCommand::Asset(AssetCommand::LoadGltf(path)));
                }
                if ui.menu_item("Add Ibl")
                    && let Some(path) = file_open(FileFilter::Hdr)
                {
                    ctx.commands
                        .send(EditorCommand::Asset(AssetCommand::AddIbl(path)));
                }
                if ui.menu_item("Clear Scene") {
                    ctx.commands.send(EditorCommand::Scene(SceneCommand::Clear));
                }
                ui.separator();
                if ui.menu_item("Exit") {
                    ctx.commands.send(EditorCommand::Exit);
                }
                ui.separator();
                ui.menu("Recent Files", || {
                    for (name, path) in self.scene_settings.recent.iter() {
                        if ui.menu_item(name) {
                            ctx.commands
                                .send(EditorCommand::Scene(SceneCommand::Open(path.into())));
                        }
                    }
                    if self.scene_settings.recent.is_empty() {
                        ui.text_disabled("No recent files");
                    }
                });
            }
            if let Some(_menu) = ui.begin_menu("Edit") {
                ui.menu_item("Undo");
                ui.menu_item("Redo");
            }
            if let Some(_menu) = ui.begin_menu("View") {
                ui.menu_item("Show Stats");
            }
        }
    }
}

pub enum FileFilter {
    Gltf,
    Json,
    Hdr,
}
impl FileFilter {
    fn as_args(&self) -> (&str, &[&str]) {
        match self {
            Self::Gltf => ("glTF", &["gltf", "glb"]),
            Self::Json => ("json", &["json"]),
            Self::Hdr => ("hdr", &["hdr"]),
        }
    }
}
pub fn file_save(filter: FileFilter) -> Option<PathBuf> {
    let (name, ext) = filter.as_args();
    rfd::FileDialog::new().add_filter(name, ext).save_file()
}
pub fn file_open(filter: FileFilter) -> Option<PathBuf> {
    let (name, ext) = filter.as_args();
    rfd::FileDialog::new().add_filter(name, ext).pick_file()
}
