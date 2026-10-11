use super::ui_layer::{Layer, UiContext};
use crate::editor::{
    AssetCommand, CameraCommand, EditorResourceStatsData, EditorSettingsData, EditorStatisticsData,
    GlobalCommand, IblData, SelectionCommand,
};
use imgui::{Drag, SliderFlags, TreeNodeFlags, Ui};

#[derive(Default)]
pub struct SettingsUi {
    settings: Option<EditorSettingsData>,
    statistics: Option<EditorStatisticsData>,
    resource_stats: Option<EditorResourceStatsData>,
    ibls: Vec<IblData>,

    demo_open: bool,
    adapter_string: String,
}

impl SettingsUi {
    pub(crate) fn new(adapter_string: String) -> Self {
        Self {
            demo_open: false,
            adapter_string,
            settings: None,
            statistics: None,
            resource_stats: None,
            ibls: Vec::new(),
        }
    }
}

impl Layer for SettingsUi {
    fn update(&mut self, commands: &super::ui_commands::UiCommands) {
        self.settings = commands.settings().cloned();
        self.statistics = commands.statistics().cloned();
        self.resource_stats = commands.resource_stats().cloned();
        self.ibls = commands.ibls().to_vec();
    }

    fn build(&mut self, ui: &Ui, ctx: &mut UiContext) {
        let Some(settings) = self.settings.clone() else {
            ui.window("Settings")
                .size([320.0, 300.0], imgui::Condition::FirstUseEver)
                .build(|| ui.text("Loading settings..."));
            return;
        };
        ui.window("Settings")
            .size([320.0, 500.0], imgui::Condition::FirstUseEver)
            .build(|| self.draw(ui, ctx, &settings));
    }
}

impl SettingsUi {
    fn draw_resource_stats(&self, ui: &Ui, stats: &EditorResourceStatsData) {
        if ui.collapsing_header("Assets", TreeNodeFlags::DEFAULT_OPEN) {
            resource_stat_row(ui, "Textures", &stats.textures);
            resource_stat_row(ui, "Materials", &stats.materials);
            resource_stat_row(ui, "Meshes", &stats.meshes);
            resource_stat_row(ui, "IBL", &stats.ibl);
        }

        if ui.collapsing_header("GPU Resources", TreeNodeFlags::DEFAULT_OPEN) {
            resource_stat_row(ui, "Textures", &stats.gpu_textures);
            resource_stat_row(ui, "Materials", &stats.gpu_materials);
            resource_stat_row(ui, "Meshes", &stats.gpu_meshes);
            resource_stat_row(ui, "Shadows", &stats.gpu_shadows);
            resource_stat_row(ui, "IBL", &stats.gpu_ibl);
        }
    }

    fn draw(&mut self, ui: &Ui, ctx: &mut UiContext, settings: &EditorSettingsData) {
        if let Some(stats) = self.resource_stats.as_ref()
            && ui.collapsing_header("Resource Stats", TreeNodeFlags::DEFAULT_OPEN)
        {
            self.draw_resource_stats(ui, stats);
        }

        if ui.collapsing_header("Statistics", TreeNodeFlags::DEFAULT_OPEN) {
            if let Some(stats) = self.statistics.as_ref() {
                ui.text(format!("FPS: {:.1}", stats.fps));
                ui.text(format!("Frametime: {:.2} ms", stats.frametime * 1000.0));
                ui.text(format!(
                    "Opaque: {} calls | {} instances",
                    stats.opaque_draw_calls, stats.opaque_instances
                ));
                ui.text(format!(
                    "Transmission: {} calls | {} instances",
                    stats.transmission_draw_calls, stats.transmission_instances
                ));
                ui.text(format!("Adapter: {}", self.adapter_string));
            } else {
                ui.text("Collecting statistics...");
            }
        }
        if ui.collapsing_header("Toggles", TreeNodeFlags::DEFAULT_OPEN) {
            toggle(
                ui,
                "Mips with CS",
                settings.mips_cp,
                ctx,
                GlobalCommand::SetMipsWithCompute,
            );
            toggle(
                ui,
                "Light",
                settings.light_enable,
                ctx,
                GlobalCommand::SetLightEnable,
            );
            toggle(
                ui,
                "IBL",
                settings.ibl_enable,
                ctx,
                GlobalCommand::SetIblEnable,
            );
            toggle(
                ui,
                "Skybox",
                settings.skybox_enable,
                ctx,
                GlobalCommand::SetSkyboxEnable,
            );
            toggle(
                ui,
                "Skybox blur",
                settings.skybox_enable_blur,
                ctx,
                GlobalCommand::SetSkyboxBlur,
            );
            toggle(
                ui,
                "Axis",
                settings.axis_enable,
                ctx,
                GlobalCommand::SetAxisEnable,
            );
            toggle(
                ui,
                "Bounding box",
                settings.bbox_enable,
                ctx,
                GlobalCommand::SetBoundingBoxEnable,
            );
            if settings.bbox_enable {
                toggle(
                    ui,
                    "Box aligned",
                    settings.bbox_axis_aligned,
                    ctx,
                    GlobalCommand::SetBoundingBoxAxisAligned,
                );
            }

            let mut exposure = settings.exposure;
            if ui
                .slider_config("Scene Exposure", 0.001, 64.0)
                .flags(SliderFlags::LOGARITHMIC)
                .build(&mut exposure)
            {
                ctx.commands.send(GlobalCommand::SetExposure(exposure));
            }

            let mut ibl_intensity = settings.ibl_intensity;
            if ui
                .slider_config("IBL Intensity", 0.01, 10_000.0)
                .flags(SliderFlags::LOGARITHMIC)
                .build(&mut ibl_intensity)
            {
                ctx.commands
                    .send(GlobalCommand::SetIblIntensity(ibl_intensity));
            }

            let mut env_rotation = settings.env_rotation;
            if ui
                .slider_config("Env rotation", 0.0, 360.0)
                .build(&mut env_rotation)
            {
                ctx.commands
                    .send(GlobalCommand::SetEnvironmentRotation(env_rotation));
            }

            const DEBUG: [&str; 18] = [
                "None",
                "TextureCoords0",
                "TextureCoords1",
                "Base Color",
                "Normal Texture",
                "Geometry Normal",
                "Geometry Tangent",
                "Geometry Bitangent",
                "Geometry Tangent W",
                "ShadingNormal",
                "Metallic",
                "Roughness",
                "Emissive",
                "Occlusion",
                "Transmission",
                "VolumeThickness",
                "SheenColor",
                "SheenRoughness",
            ];
            let mut debug = settings.debug_code as usize;
            if ui.combo("Debug Mode", &mut debug, &DEBUG, |item| {
                std::borrow::Cow::Borrowed(*item)
            }) {
                ctx.commands.send(GlobalCommand::SetDebugCode(debug as u32));
            }

            const TONEMAP: [&str; 9] = [
                "Khronos PBR Neutral",
                "ACES",
                "Filmic",
                "Lottes",
                "Reinhard",
                "Reinhard2",
                "Uchimura",
                "Uncharted2",
                "Exponential",
            ];
            let mut tonemap = settings.tonemap_filter as usize;
            if ui.combo("Tonemap", &mut tonemap, &TONEMAP, |item| {
                std::borrow::Cow::Borrowed(*item)
            }) {
                ctx.commands.send(GlobalCommand::SetTonemap(tonemap as u32));
            }

            ui.checkbox("Show demo window", &mut self.demo_open);
            if self.demo_open {
                ui.show_demo_window(&mut self.demo_open);
            }
        }

        if ui.collapsing_header("IBL", TreeNodeFlags::DEFAULT_OPEN) {
            if self.ibls.is_empty() {
                ui.text("No IBL loaded");
            } else {
                for ibl in &self.ibls {
                    let Some(texture) = ctx.textures.asset(ibl.texture) else {
                        continue;
                    };

                    let _style = ibl.selected.then(|| push_selected_button_style(ui));
                    let label = format!("##ibl_{:?}", ibl.id);

                    if ui.image_button(&label, texture, [60.0, 60.0]) {
                        ctx.commands
                            .send(SelectionCommand::SelectIbl { id: ibl.id });
                    }
                }
            }

            if ui.button("Add IBL")
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("hdr", &["hdr"])
                    .pick_file()
            {
                ctx.commands.send(AssetCommand::AddIbl(path));
            }
            ui.separator();
        }

        if ui.collapsing_header("Camera", TreeNodeFlags::DEFAULT_OPEN) {
            ui.text(format!("FOV: {:.1}", settings.camera_fov));
            if ui.button("Recenter") {
                ctx.commands.send(CameraCommand::Recenter);
            }

            let mut fov = settings.camera_fov;
            if Drag::new("FOV")
                .range(1.0, 179.0)
                .speed(1.0)
                .build(ui, &mut fov)
            {
                ctx.commands.send(CameraCommand::SetFov(fov));
            }

            let mut distance = settings.camera_distance;
            if Drag::new("Distance")
                .range(0.0, f32::MAX)
                .speed(1.0)
                .build(ui, &mut distance)
            {
                ctx.commands.send(CameraCommand::SetDistance(distance));
            }

            let mut near = settings.camera_near;
            let mut far = settings.camera_far;
            if Drag::new("Near")
                .range(0.01, f32::MAX)
                .speed(0.1)
                .build(ui, &mut near)
            {
                ctx.commands.send(CameraCommand::SetNearFar { near, far });
            }
            if Drag::new("Far")
                .range(0.1, f32::MAX)
                .speed(1.0)
                .build(ui, &mut far)
            {
                ctx.commands.send(CameraCommand::SetNearFar { near, far });
            }
        }
    }
}

fn push_selected_button_style(
    ui: &Ui,
) -> (
    imgui::ColorStackToken<'_>,
    imgui::ColorStackToken<'_>,
    imgui::ColorStackToken<'_>,
) {
    (
        ui.push_style_color(imgui::StyleColor::Button, [0.2, 0.5, 1.0, 1.0]),
        ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.3, 0.6, 1.0, 1.0]),
        ui.push_style_color(imgui::StyleColor::ButtonActive, [0.1, 0.4, 0.9, 1.0]),
    )
}

fn toggle<F: FnOnce(bool) -> GlobalCommand>(
    ui: &Ui,
    label: &str,
    value: bool,
    ctx: &mut UiContext,
    make: F,
) {
    let mut value = value;
    if ui.checkbox(label, &mut value) {
        ctx.commands.send(make(value));
    }
}

fn resource_stat_row(ui: &Ui, label: &str, stats: &crate::editor::ResourceStatsData) {
    ui.text(format!(
        "{label}: {} ({})",
        stats.count,
        format_bytes(stats.estimated_bytes)
    ));
}

fn format_bytes(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GiB", value / GB)
    } else if value >= MB {
        format!("{:.2} MiB", value / MB)
    } else if value >= KB {
        format!("{:.2} KiB", value / KB)
    } else {
        format!("{bytes} B")
    }
}
