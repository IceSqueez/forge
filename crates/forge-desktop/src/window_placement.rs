use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_storage::{SettingsRepo, reserved_keys, set_json_setting};
use gpui::{
    App, AppContext, Bounds, Context, DisplayId, Entity, Global, Pixels, PlatformDisplay,
    Subscription, Task, Window, WindowBounds, point, px, size,
};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(250);
const WAYLAND_COMPOSITOR: &str = "Wayland";
const FIRST_LAUNCH_SIZE: gpui::Size<Pixels> = size(px(1080.0), px(720.0));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    Windowed,
    Maximized,
    Fullscreen,
}

impl WindowMode {
    fn with_restore_bounds(self, bounds: Bounds<Pixels>) -> WindowBounds {
        match self {
            Self::Windowed => WindowBounds::Windowed(bounds),
            Self::Maximized => WindowBounds::Maximized(bounds),
            Self::Fullscreen => WindowBounds::Fullscreen(bounds),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowPlacement {
    pub mode: WindowMode,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub display_uuid: Option<String>,
}

impl WindowPlacement {
    pub fn capture(bounds: WindowBounds, display_uuid: Option<String>) -> Self {
        let (mode, rect) = match bounds {
            WindowBounds::Windowed(rect) => (WindowMode::Windowed, rect),
            WindowBounds::Maximized(rect) => (WindowMode::Maximized, rect),
            WindowBounds::Fullscreen(rect) => (WindowMode::Fullscreen, rect),
        };
        Self {
            mode,
            x: rect.origin.x.as_f32(),
            y: rect.origin.y.as_f32(),
            width: rect.size.width.as_f32(),
            height: rect.size.height.as_f32(),
            display_uuid,
        }
    }

    fn restore_rect(&self) -> Option<Bounds<Pixels>> {
        let finite = [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite());
        (finite && self.width > 0.0 && self.height > 0.0).then(|| {
            Bounds::new(
                point(px(self.x), px(self.y)),
                size(px(self.width), px(self.height)),
            )
        })
    }
}

pub struct DisplayArea {
    pub id: DisplayId,
    pub uuid: Option<String>,
    pub bounds: Bounds<Pixels>,
    pub visible: Bounds<Pixels>,
}

impl DisplayArea {
    fn of(display: &dyn PlatformDisplay) -> Self {
        Self {
            id: display.id(),
            uuid: display_uuid(display),
            bounds: display.bounds(),
            visible: display.visible_bounds(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct RestoredPlacement {
    pub bounds: WindowBounds,
    pub display_id: Option<DisplayId>,
}

pub fn restore_placement(
    saved: &WindowPlacement,
    displays: &[DisplayArea],
    origin_honored: bool,
) -> Option<RestoredPlacement> {
    let rect = saved.restore_rect()?;
    let saved_display = saved.display_uuid.as_deref().and_then(|uuid| {
        displays
            .iter()
            .find(|display| display.uuid.as_deref() == Some(uuid))
    });
    if origin_honored {
        let display = saved_display?;
        if !rect.intersects(&display.bounds) {
            return None;
        }
        return Some(RestoredPlacement {
            bounds: saved
                .mode
                .with_restore_bounds(clamp_into(rect, display.visible)),
            display_id: Some(display.id),
        });
    }
    let size_limit = saved_display
        .or_else(|| displays.first())
        .map(|display| display.visible.size);
    let restored_size = size_limit.map_or(rect.size, |limit| rect.size.min(&limit));
    Some(RestoredPlacement {
        bounds: saved
            .mode
            .with_restore_bounds(Bounds::new(point(px(0.0), px(0.0)), restored_size)),
        display_id: None,
    })
}

fn clamp_into(rect: Bounds<Pixels>, area: Bounds<Pixels>) -> Bounds<Pixels> {
    let fitted = rect.size.min(&area.size);
    let furthest_x = area.origin.x + area.size.width - fitted.width;
    let furthest_y = area.origin.y + area.size.height - fitted.height;
    Bounds::new(
        point(
            rect.origin.x.min(furthest_x).max(area.origin.x),
            rect.origin.y.min(furthest_y).max(area.origin.y),
        ),
        fitted,
    )
}

fn display_uuid(display: &dyn PlatformDisplay) -> Option<String> {
    display.uuid().ok().map(|uuid| uuid.to_string())
}

fn compositor_is_wayland(cx: &App) -> bool {
    cx.compositor_name() == WAYLAND_COMPOSITOR
}

pub fn initial_window_bounds(
    saved: Option<&WindowPlacement>,
    cx: &App,
) -> (WindowBounds, Option<DisplayId>) {
    let displays: Vec<DisplayArea> = cx
        .displays()
        .iter()
        .map(|display| DisplayArea::of(display.as_ref()))
        .collect();
    let origin_honored = !compositor_is_wayland(cx);
    match saved.and_then(|saved| restore_placement(saved, &displays, origin_honored)) {
        Some(restored) => (restored.bounds, restored.display_id),
        None => (
            WindowBounds::Windowed(Bounds::centered(None, FIRST_LAUNCH_SIZE, cx)),
            None,
        ),
    }
}

struct WindowPlacementTracking {
    _recorder: Entity<WindowPlacementRecorder>,
}

impl Global for WindowPlacementTracking {}

pub fn track_window_placement(
    window: &mut Window,
    settings: Arc<dyn SettingsRepo>,
    rt_handle: Handle,
    persisted: Option<WindowPlacement>,
    cx: &mut App,
) {
    let recorder =
        cx.new(|cx| WindowPlacementRecorder::new(window, settings, rt_handle, persisted, cx));
    cx.set_global(WindowPlacementTracking {
        _recorder: recorder,
    });
}

pub struct WindowPlacementRecorder {
    settings: Arc<dyn SettingsRepo>,
    rt_handle: Handle,
    latest: Option<WindowPlacement>,
    persisted: Option<WindowPlacement>,
    pending_save: Option<Task<()>>,
    _subscriptions: [Subscription; 2],
}

impl WindowPlacementRecorder {
    fn new(
        window: &mut Window,
        settings: Arc<dyn SettingsRepo>,
        rt_handle: Handle,
        persisted: Option<WindowPlacement>,
        cx: &mut Context<Self>,
    ) -> Self {
        let bounds_changed =
            cx.observe_window_bounds(window, |this, window, cx| this.record(window, cx));
        let quitting = cx.on_app_quit(|this, _cx| this.flush());
        Self {
            settings,
            rt_handle,
            latest: None,
            persisted,
            pending_save: None,
            _subscriptions: [bounds_changed, quitting],
        }
    }

    fn record(&mut self, window: &Window, cx: &mut Context<Self>) {
        let display_uuid = window
            .display(cx)
            .and_then(|display| display_uuid(display.as_ref()));
        self.latest = Some(WindowPlacement::capture(
            window.inner_window_bounds(),
            display_uuid,
        ));
        if self.pending_save.is_some() {
            return;
        }
        self.pending_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            this.update(cx, |this, _cx| {
                this.pending_save = None;
                this.persist_latest();
            })
            .ok();
        }));
    }

    fn persist_latest(&mut self) -> Option<JoinHandle<()>> {
        let latest = self.latest.clone()?;
        if self.persisted.as_ref() == Some(&latest) {
            return None;
        }
        self.persisted = Some(latest.clone());
        let settings = Arc::clone(&self.settings);
        Some(self.rt_handle.spawn(async move {
            if let Err(err) =
                set_json_setting(settings.as_ref(), reserved_keys::WINDOW_PLACEMENT, &latest).await
            {
                tracing::warn!(error = %err, "failed to persist window placement");
            }
        }))
    }

    fn flush(&mut self) -> impl Future<Output = ()> + use<> {
        self.pending_save = None;
        let write = self.persist_latest();
        async move {
            if let Some(write) = write {
                let _ = write.await;
            }
        }
    }
}
