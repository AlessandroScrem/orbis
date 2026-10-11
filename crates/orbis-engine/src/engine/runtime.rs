use super::RuntimeEvent;
use super::gpu_sync::TextureLoadService;
use crate::app::Application;
use crate::app::domain::events::CameraEvent::{CameraOrbit, CameraPan, CameraZoom};
use crate::app::domain::events::DomainEvent::Camera;
use crate::assets::asset_manager::AssetManager;
use crate::assets::{IblId, TextureId};
use crate::editor::EditorConnection;
use crate::engine::editor::{EditorBackend, EditorService};
use crate::engine::engine::EventBus;
use crate::engine::frame_preparation;
use crate::engine::picking::PickingService;
use crate::engine::presentation;
use crate::engine::readback::ReadbackManager;
use crate::engine::ui_statistics::UiStatistics;
use crate::gpu::pipeline_manager::PipelineManager;
use crate::gpu::{
    GpuCache, GpuContext, GpuManager, GpuMaterialCache, GpuMeshCache, GpuSurface, GpuTextureCache,
    IblManager, ShadowManager,
};
use crate::input::Input;
use crate::prelude::info;
use crate::renderer::ImguiRender;
use crate::renderer::scene_renderer::SceneRenderContext;
use crate::renderer::{MaterialPreviewRenderer, SceneRenderer};
use crate::ui::UiLayer;
use crate::winit_bridge::WindowHandle;

pub struct Runtime {
    pub window: WindowHandle,
    pub gpu_context: GpuContext,
    pub gpu_surface: GpuSurface,
    pub gpu_cache: GpuCache,
    pub gpu_manager: GpuManager,
    pub ibl_manager: IblManager,
    pub pipeline_manager: PipelineManager,
    pub shadow_manager: ShadowManager,
    readback: ReadbackManager,
    picking: PickingService,
    pub uilayer: UiLayer,
    pub input: Input,
    pub scene_renderer: SceneRenderer,
    pub material_preview_renderer: MaterialPreviewRenderer,
    pub imgui_render: ImguiRender,
    pub hdr_vec: Vec<(TextureId, IblId)>,
    pub wait_for_exit: bool,
    pub editor_service: EditorService,
    texture_loader: TextureLoadService,
    ui_statistics: UiStatistics,
}

impl Runtime {
    pub(crate) fn new(window: WindowHandle) -> Self {
        let mut imgui_context = imgui::Context::create();
        let gpu_context = GpuContext::default();
        let gpu_surface = GpuSurface::new(
            gpu_context.adapter(),
            gpu_context.instance(),
            window.clone(),
        );
        let mut imgui_render = ImguiRender::new(
            &gpu_context.device,
            &gpu_context.queue,
            &window,
            &mut imgui_context,
            gpu_surface.get_config().format,
        );
        let gpu_cache = GpuCache {
            textures: GpuTextureCache::new(&gpu_context.as_ref()),
            material: GpuMaterialCache::default(),
            mesh: GpuMeshCache::default(),
        };
        let (width, height) = (
            gpu_surface.get_config().width,
            gpu_surface.get_config().height,
        );
        let gpu_manager = GpuManager::new(&gpu_context.as_ref(), width, height);
        let shadow_manager = ShadowManager::new(&gpu_context.as_ref());
        imgui_render.registry.add(
            &mut imgui_render.renderer,
            &gpu_context.device,
            shadow_manager.get_rgba_id(),
            shadow_manager.get_rgba(),
        );
        let ibl_manager = IblManager::new(&gpu_context.as_ref());
        let pipeline_manager = PipelineManager::new(
            &gpu_context.device,
            &gpu_manager,
            gpu_surface.get_config().format,
        );
        let scene_renderer = SceneRenderer::new();
        let (connection, channels) = EditorConnection::new();
        let editor_service = EditorService::new(channels);
        let uilayer = UiLayer::new(
            &window,
            imgui_context,
            gpu_context.get_adapter_string(),
            connection,
        );
        Self {
            window: window.clone(),
            input: Input::new(),
            scene_renderer,
            material_preview_renderer: MaterialPreviewRenderer::new(&gpu_context.device),
            imgui_render,
            uilayer,
            gpu_context,
            gpu_surface,
            gpu_cache,
            gpu_manager,
            ibl_manager,
            pipeline_manager,
            shadow_manager,
            hdr_vec: Vec::new(),
            wait_for_exit: false,
            readback: ReadbackManager::default(),
            picking: PickingService::default(),
            editor_service,
            texture_loader: TextureLoadService::new(),
            ui_statistics: UiStatistics::default(),
        }
    }

    pub fn handle_input(&mut self, bus: &mut EventBus) {
        use crate::input::MouseButton;
        let entity_id_texture = self
            .gpu_manager
            .get_framebuffer_texture(crate::gpu::FramebufferKind::EntityId);
        self.picking.handle_input(
            &mut self.readback,
            &self.input,
            &self.gpu_context.as_ref(),
            entity_id_texture,
            bus,
            &self.editor_service,
        );
        let input = &self.input;
        if input.is_mouse_dragging(MouseButton::Left) && input.any_key_down() {
            bus.send_domain(Camera(CameraOrbit(
                input.mouse_delta.x as f64,
                input.mouse_delta.y as f64,
            )));
        }
        if input.is_mouse_dragging(MouseButton::Middle) {
            bus.send_domain(Camera(CameraPan(
                input.mouse_delta.x as f64,
                input.mouse_delta.y as f64,
            )));
        }
        if let Some(delta) = input.mouse_wheel_movement {
            bus.send_domain(Camera(CameraZoom(delta.y)));
        }
        self.input.clear();
    }

    pub fn handle_runtime_events<A: Application>(
        &mut self,
        app: &mut A,
        bus: &mut EventBus,
    ) -> Option<String> {
        let mut window_title = None;
        for event in bus.drain_runtime() {
            match event {
                RuntimeEvent::Resize { width, height } => {
                    if width == 0 || height == 0 {
                        return window_title;
                    }
                    presentation::resize(
                        &self.gpu_context,
                        &mut self.gpu_surface,
                        &mut self.gpu_manager,
                        width,
                        height,
                    );
                    app.on_resize(width, height);
                }
                RuntimeEvent::CloseRequested => {
                    app.on_close();
                    self.wait_for_exit = true;
                }
                RuntimeEvent::DroppedFile(path) => app.on_drop(path, bus),
                RuntimeEvent::SetWindowTitle(title) => {
                    window_title = Some(title);
                    info!("Set window title");
                }
                RuntimeEvent::UpdateIblMaps(id) => {
                    self.material_preview_renderer.invalidate_environment();
                    self.gpu_manager.replace_pbrmap_skybox_bindgroup(
                        self.ibl_manager.get(&id),
                        &self.shadow_manager,
                        &self.gpu_context.device,
                    );
                }
                RuntimeEvent::ClearIblMaps => {
                    self.material_preview_renderer.invalidate_environment();
                    self.gpu_manager
                        .clear_ibl_bindgroups(&self.gpu_context.as_ref());
                }
                RuntimeEvent::ReadbackSelection(pos, size) => {
                    PickingService::request_selection(
                        &mut self.readback,
                        &self.gpu_context.as_ref(),
                        &self
                            .gpu_manager
                            .get_framebuffer_texture(crate::gpu::FramebufferKind::EntityId),
                        pos,
                        size,
                    );
                }
            }
        }
        window_title
    }

    pub fn sync_gpu_assets(&mut self, asset_mgr: &mut AssetManager, bus: &mut EventBus) {
        super::gpu_sync::sync_gpu_assets(
            asset_mgr,
            bus,
            &self.gpu_context,
            &mut self.gpu_cache,
            &mut self.texture_loader,
            &mut self.gpu_manager,
            &mut self.ibl_manager,
            &mut self.imgui_render,
            &mut self.hdr_vec,
            &mut self.material_preview_renderer,
        );
    }

    pub fn update_ui<A: Application + EditorBackend>(&mut self, app: &mut A, bus: &mut EventBus) {
        let asset_mgr = app.render_data().asset_mgr;
        self.ui_statistics.update(
            &mut self.editor_service,
            &self.scene_renderer,
            asset_mgr,
            &self.gpu_cache,
            &self.shadow_manager,
            &self.ibl_manager,
        );
        self.editor_service.process(app, bus);
        let textures = self.imgui_render.registry.ui_textures(
            self.shadow_manager.get_rgba_id(),
            self.material_preview_renderer.resource_id(),
        );
        let output = self.uilayer.build(&self.window, &textures);
        self.material_preview_renderer
            .set_material(output.material_preview);
    }

    pub fn render<A: Application>(&mut self, app: &A) {
        let mut encoder = self.gpu_context.create_encoder();
        if let Some(frame) = self.gpu_surface.get_frame() {
            let target = frame.texture.create_view(&Default::default());
            let frame_data = frame_preparation::prepare_frame_data(
                app.render_data(),
                &self.gpu_context,
                &self.gpu_surface,
                &self.gpu_cache,
                &self.texture_loader,
                &self.gpu_manager,
            );
            let context = SceneRenderContext {
                gpu_context: &self.gpu_context,
                gpu_manager: &self.gpu_manager,
                shadow_manager: &self.shadow_manager,
                pipeline_manager: &self.pipeline_manager,
                gpu_cache: &self.gpu_cache,
            };
            self.scene_renderer
                .render(&context, &mut encoder, &target, &frame_data);

            if let Some(preview_texture) = self.material_preview_renderer.render(
                &mut encoder,
                &self.gpu_context,
                &self.gpu_manager,
                &self.gpu_cache,
                &self.pipeline_manager,
            ) {
                self.imgui_render.registry.add(
                    &mut self.imgui_render.renderer,
                    &self.gpu_context.device,
                    self.material_preview_renderer.resource_id(),
                    &preview_texture,
                );
            }

            self.imgui_render.render(
                self.uilayer.get_draw_data(),
                &mut encoder,
                &target,
                &self.gpu_context.device,
                &self.gpu_context.queue,
            );
            self.gpu_context.queue.submit([encoder.finish()]);
            frame.present();
        }
    }
}
