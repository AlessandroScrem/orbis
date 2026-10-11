use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, Event, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

use crate::app::RuntimeApp;
use crate::engine::{Engine, RuntimeEvent};

pub(crate) type WindowHandle = Arc<Window>;

#[derive(Default)]
pub(crate) struct MyApplication<A: RuntimeApp + Default> {
    engine: Engine<A>,
    size: PhysicalSize<u32>,
}

impl<A: RuntimeApp + Default> MyApplication<A> {
    pub(crate) fn new_with_size(width: u32, height: u32) -> Self {
        Self {
            size: PhysicalSize::new(width, height),
            ..Default::default()
        }
    }

    pub(crate) fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run_app(&mut self)?;
        Ok(())
    }
}

pub(crate) trait CenterWindow {
    fn try_fit_center_to_monitor(self) -> Self;
}

impl CenterWindow for WindowHandle {
    fn try_fit_center_to_monitor(self) -> Self {
        if let Some(monitor) = self.current_monitor() {
            let screen_size = monitor.size();
            let window_size = self.inner_size();
            let safe_width = screen_size.width.min(window_size.width);
            let safe_height = screen_size.height.min(window_size.height);
            let x = (screen_size.width.saturating_sub(safe_width)) as f32 / 2.0;
            let y = (screen_size.height.saturating_sub(safe_height)) as f32 / 2.0;
            self.set_outer_position(PhysicalPosition::new(x, y));
        }
        self
    }
}

fn load_icon(bytes: &[u8]) -> Option<winit::window::Icon> {
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), width, height).ok()
}

fn create_window(event_loop: &ActiveEventLoop, size: PhysicalSize<u32>) -> WindowHandle {
    let icon = load_icon(include_bytes!(crate::asset_path!("core/icon.ico")));

    let attrs = WindowAttributes::default()
        .with_inner_size(size)
        .with_window_icon(icon)
        .with_title("Orbis");

    Arc::new(
        event_loop
            .create_window(attrs)
            .expect("Failed to create window"),
    )
    .try_fit_center_to_monitor()
}

fn is_minimized(window: &WindowHandle) -> bool {
    window.is_minimized().unwrap_or(false)
}

pub(crate) fn set_window_title(window: &WindowHandle, title: &str) {
    window.set_title(title);
}

impl<A: RuntimeApp + Default> ApplicationHandler for MyApplication<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.engine.runtime.is_some() {
            return;
        }

        let window = create_window(event_loop, self.size);
        self.engine.resume(window.clone());
        window.request_redraw();
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        let Some(runtime) = &mut self.engine.runtime else {
            return;
        };

        let event: Event<()> = Event::DeviceEvent { device_id, event };
        runtime.uilayer.handle_event(&runtime.window, &event);
        if !runtime.uilayer.want_capture_mouse() {
            runtime.input.update_events(&event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(runtime) = &self.engine.runtime else {
            return;
        };

        if runtime.wait_for_exit {
            event_loop.exit();
            return;
        }

        if is_minimized(&runtime.window) {
            return;
        }

        runtime.window.request_redraw();
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self
            .engine
            .runtime
            .as_ref()
            .map(|runtime| runtime.window.clone())
        else {
            return;
        };

        {
            let Some(runtime) = &mut self.engine.runtime else {
                return;
            };

            let evt: Event<()> = Event::WindowEvent {
                window_id,
                event: event.clone(),
            };
            runtime.uilayer.handle_event(&runtime.window, &evt);
            if !runtime.uilayer.want_capture_mouse() {
                runtime.input.update_events(&evt);
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                self.engine.bus.send_runtime(RuntimeEvent::CloseRequested);
            }
            WindowEvent::Resized(size) => {
                self.engine.bus.send_runtime(RuntimeEvent::Resize {
                    width: size.width,
                    height: size.height,
                });
            }
            WindowEvent::RedrawRequested => {
                if !is_minimized(&window)
                    && let Some(title) = self.engine.tick(false)
                {
                    set_window_title(&window, &title);
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.engine
                    .bus
                    .send_runtime(RuntimeEvent::DroppedFile(path));
            }
            _ => {}
        }
    }
}
