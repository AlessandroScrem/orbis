use std::collections::VecDeque;

use super::Runtime;
use crate::app::RuntimeApp;
use crate::app::domain::events::DomainEvent;
use crate::engine::RuntimeEvent;
use crate::prelude::*;

pub struct EventBus {
    domain: VecDeque<DomainEvent>,
    runtime: VecDeque<RuntimeEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            domain: VecDeque::new(),
            runtime: VecDeque::new(),
        }
    }

    pub fn send_domain(&mut self, event: DomainEvent) {
        self.domain.push_back(event);
    }

    pub fn send_runtime(&mut self, event: RuntimeEvent) {
        self.runtime.push_back(event);
    }

    pub fn drain_domain(&mut self) -> Vec<DomainEvent> {
        self.domain.drain(..).collect()
    }

    pub fn drain_runtime(&mut self) -> Vec<RuntimeEvent> {
        self.runtime.drain(..).collect()
    }
}

#[derive(Default)]
pub struct Engine<A: RuntimeApp + Default> {
    pub app: A,
    pub runtime: Option<Runtime>,
    pub bus: EventBus,
}

impl<A: RuntimeApp + Default> Engine<A> {
    pub(crate) fn resume(&mut self, window: crate::winit_bridge::WindowHandle) {
        if self.runtime.is_some() {
            return;
        }

        debug!("App resumed");
        let Self { app, bus, .. } = self;
        app.init(bus);
        self.runtime = Some(Runtime::new(window));
    }

    /// Runs one frame in this order:
    /// 1. Collect input and process runtime events.
    /// 2. If minimized, stop before application/GPU work.
    /// 3. Update application state, synchronize GPU assets, update UI, and render.
    ///
    /// Window-system APIs remain in `winit_bridge`; this method only coordinates
    /// the engine and runtime layers.
    pub(crate) fn tick(&mut self, minimized: bool) -> Option<String> {
        let Self { app, bus, runtime } = self;
        let Some(runtime) = runtime else {
            return None;
        };

        let window_title = Self::process_frame_events(app, bus, runtime);
        if minimized {
            return window_title;
        }

        Self::update_and_render_frame(app, bus, runtime);
        window_title
    }

    fn process_frame_events(
        app: &mut A,
        bus: &mut EventBus,
        runtime: &mut Runtime,
    ) -> Option<String> {
        runtime.handle_input(bus);
        runtime.handle_runtime_events(app, bus)
    }

    fn update_and_render_frame(app: &mut A, bus: &mut EventBus, runtime: &mut Runtime) {
        app.on_update(bus);
        runtime.sync_gpu_assets(app.asset_mgr_mut(), bus);
        runtime.update_ui(app, bus);
        runtime.render(app);
    }
}
