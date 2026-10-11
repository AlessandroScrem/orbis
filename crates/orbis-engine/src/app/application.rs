use crate::assets::asset_manager::AssetManager;
use crate::engine::editor::EditorBackend;
use crate::engine::core::EventBus;
use crate::renderer::render_objects::RenderObjects;
use legion::Entity;
use std::path::PathBuf;

pub struct AppRenderData<'a> {
    pub render_objects: &'a RenderObjects,
    pub asset_mgr: &'a AssetManager,
    pub camera: &'a crate::Camera,
    pub globals: &'a crate::Globals,
    pub selected: Option<Entity>,
}

pub trait HasAssetMgr {
    fn asset_mgr_mut(&mut self) -> &mut AssetManager;
}

pub trait RuntimeApp: Application + HasAssetMgr + EditorBackend {}
impl<T> RuntimeApp for T where T: Application + HasAssetMgr + EditorBackend {}

pub trait Application {
    fn init(&mut self, bus: &mut EventBus);
    fn render_data(&self) -> AppRenderData<'_>;
    fn on_update(&mut self, bus: &mut EventBus);
    fn on_resize(&mut self, width: u32, height: u32);
    fn on_drop(&mut self, path: PathBuf, bus: &mut EventBus);
    fn on_close(&mut self);
}
