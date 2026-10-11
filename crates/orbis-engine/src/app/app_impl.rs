use super::{App, Application, HasAssetMgr};
use crate::app::Settings;
use crate::app::application::AppRenderData;
use crate::app::domain::events::SelectionEvent::SelectIbl;
use crate::app::domain::events::{AssetEvent, DomainEvent, SceneEvent};
use crate::assets::TextureAsset;
use crate::assets::asset_manager::AssetManager;
use crate::assets::ibl_asset::IblAsset;

use crate::engine::core::EventBus;
use crate::prelude::*;

impl HasAssetMgr for App {
    fn asset_mgr_mut(&mut self) -> &mut AssetManager {
        &mut self.asset_mgr
    }
}
impl Application for App {
    fn init(&mut self, bus: &mut EventBus) {
        let timer = std::time::Instant::now();
        self.settings = Settings::load();
        crate::ecs::components::light::new(&mut self.current_scene.world);
        const HDRPATH: &str = crate::asset_path!("core/Cannon_Exterior.hdr");
        let hdr_texture_asset =
            TextureAsset::from_file(HDRPATH, crate::assets::texture_asset::TextureUsage::HDR16);
        let hdr_id = self.asset_mgr.add::<TextureAsset>(hdr_texture_asset);
        let ibl_id = self
            .asset_mgr
            .add::<IblAsset>(IblAsset::new(hdr_id, HDRPATH));
        bus.send_domain(DomainEvent::Selection(SelectIbl(ibl_id)));
        debug!("App initialized in {} ms", timer.elapsed().as_millis());
    }

    fn on_update(&mut self, bus: &mut EventBus) {
        self.update_domain_event(bus);
        self.current_scene.update_scene(bus, &self.globals);
    }

    fn on_resize(&mut self, width: u32, height: u32) {
        self.camera
            .set_aspect(width.max(1) as f32 / height.max(1) as f32);
    }

    fn on_drop(&mut self, path: std::path::PathBuf, bus: &mut EventBus) {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => bus.send_domain(DomainEvent::Scene(SceneEvent::Open(path))),
            Some("gltf") | Some("glb") => {
                bus.send_domain(DomainEvent::Assets(AssetEvent::LoadGltf(path)))
            }
            _ => {}
        }
    }

    fn render_data(&self) -> AppRenderData<'_> {
        AppRenderData {
            render_objects: &self.current_scene.render_objects,
            asset_mgr: &self.asset_mgr,
            camera: &self.camera,
            globals: &self.globals,
            selected: if self.selected.len() == 1 {
                self.selected.iter().next().copied()
            } else {
                None
            },
        }
    }

    fn on_close(&mut self) {
        let _ = self.settings.save();
        info!("Exit requested; App stopping");
    }
}
