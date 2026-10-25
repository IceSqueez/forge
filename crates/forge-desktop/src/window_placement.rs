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

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY_ID: u64 = 1;
    const SECONDARY_ID: u64 = 2;
    const PRIMARY_UUID: &str = "primary-display";
    const SECONDARY_UUID: &str = "secondary-display";
    const UNKNOWN_UUID: &str = "unplugged-display";
    const PRIMARY_WIDTH: f32 = 1920.0;
    const PRIMARY_HEIGHT: f32 = 1080.0;
    const PRIMARY_TOP_BAR: f32 = 32.0;
    const SECONDARY_WIDTH: f32 = 2560.0;
    const SECONDARY_HEIGHT: f32 = 1440.0;
    const WINDOW_WIDTH: f32 = 800.0;
    const WINDOW_HEIGHT: f32 = 600.0;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    fn primary() -> DisplayArea {
        DisplayArea {
            id: DisplayId::new(PRIMARY_ID),
            uuid: Some(PRIMARY_UUID.to_string()),
            bounds: rect(0.0, 0.0, PRIMARY_WIDTH, PRIMARY_HEIGHT),
            visible: rect(
                0.0,
                PRIMARY_TOP_BAR,
                PRIMARY_WIDTH,
                PRIMARY_HEIGHT - PRIMARY_TOP_BAR,
            ),
        }
    }

    fn secondary() -> DisplayArea {
        let area = rect(PRIMARY_WIDTH, 0.0, SECONDARY_WIDTH, SECONDARY_HEIGHT);
        DisplayArea {
            id: DisplayId::new(SECONDARY_ID),
            uuid: Some(SECONDARY_UUID.to_string()),
            bounds: area,
            visible: area,
        }
    }

    fn two_displays() -> Vec<DisplayArea> {
        vec![primary(), secondary()]
    }

    fn placement(
        mode: WindowMode,
        bounds: Bounds<Pixels>,
        display_uuid: Option<&str>,
    ) -> WindowPlacement {
        WindowPlacement {
            mode,
            x: bounds.origin.x.as_f32(),
            y: bounds.origin.y.as_f32(),
            width: bounds.size.width.as_f32(),
            height: bounds.size.height.as_f32(),
            display_uuid: display_uuid.map(str::to_string),
        }
    }

    fn windowed(bounds: Bounds<Pixels>, display_uuid: Option<&str>) -> WindowPlacement {
        placement(WindowMode::Windowed, bounds, display_uuid)
    }

    fn restored_windowed(bounds: Bounds<Pixels>, display_id: u64) -> Option<RestoredPlacement> {
        Some(RestoredPlacement {
            bounds: WindowBounds::Windowed(bounds),
            display_id: Some(DisplayId::new(display_id)),
        })
    }

    #[test]
    fn restore_with_honored_origin_places_window_on_its_saved_display() {
        let partly_off_right_x = PRIMARY_WIDTH - WINDOW_WIDTH / 2.0;
        let one_pixel_overlap_x = PRIMARY_WIDTH - 1.0;
        let flush_right_x = PRIMARY_WIDTH - WINDOW_WIDTH;
        let oversized_width = SECONDARY_WIDTH + WINDOW_WIDTH;
        let oversized_height = SECONDARY_HEIGHT + WINDOW_HEIGHT;
        let cases = [
            (
                "inside the secondary display keeps exact bounds",
                windowed(
                    rect(2000.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(SECONDARY_UUID),
                ),
                restored_windowed(
                    rect(2000.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    SECONDARY_ID,
                ),
            ),
            (
                "partly off the right edge is pulled back inside",
                windowed(
                    rect(partly_off_right_x, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(PRIMARY_UUID),
                ),
                restored_windowed(
                    rect(flush_right_x, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    PRIMARY_ID,
                ),
            ),
            (
                "one pixel of overlap still restores on the display",
                windowed(
                    rect(one_pixel_overlap_x, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(PRIMARY_UUID),
                ),
                restored_windowed(
                    rect(flush_right_x, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    PRIMARY_ID,
                ),
            ),
            (
                "under the top bar is pushed down into the visible area",
                windowed(
                    rect(100.0, 0.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(PRIMARY_UUID),
                ),
                restored_windowed(
                    rect(100.0, PRIMARY_TOP_BAR, WINDOW_WIDTH, WINDOW_HEIGHT),
                    PRIMARY_ID,
                ),
            ),
            (
                "partly off the top left corner is pulled back inside",
                windowed(
                    rect(-100.0, -100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(PRIMARY_UUID),
                ),
                restored_windowed(
                    rect(0.0, PRIMARY_TOP_BAR, WINDOW_WIDTH, WINDOW_HEIGHT),
                    PRIMARY_ID,
                ),
            ),
            (
                "larger than the visible area is fitted to it",
                windowed(
                    rect(PRIMARY_WIDTH, 0.0, oversized_width, oversized_height),
                    Some(SECONDARY_UUID),
                ),
                restored_windowed(
                    rect(PRIMARY_WIDTH, 0.0, SECONDARY_WIDTH, SECONDARY_HEIGHT),
                    SECONDARY_ID,
                ),
            ),
        ];
        for (name, saved, expected) in cases {
            assert_eq!(
                restore_placement(&saved, &two_displays(), true),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn restore_with_honored_origin_rejects_unplaceable_saved_display() {
        let cases = [
            (
                "rect lies on another display than the saved one",
                windowed(
                    rect(2000.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(PRIMARY_UUID),
                ),
            ),
            (
                "rect only touches the saved display edge",
                windowed(
                    rect(PRIMARY_WIDTH, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(PRIMARY_UUID),
                ),
            ),
            (
                "saved display is no longer connected",
                windowed(
                    rect(100.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(UNKNOWN_UUID),
                ),
            ),
            (
                "no saved display",
                windowed(rect(100.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT), None),
            ),
        ];
        for (name, saved) in cases {
            assert_eq!(
                restore_placement(&saved, &two_displays(), true),
                None,
                "{name}"
            );
        }
    }

    #[test]
    fn restore_without_honored_origin_zeroes_origin_and_fits_size() {
        let wider_than_primary = PRIMARY_WIDTH + WINDOW_WIDTH / 2.0;
        let taller_than_primary = PRIMARY_HEIGHT;
        let primary_visible_height = PRIMARY_HEIGHT - PRIMARY_TOP_BAR;
        let cases = [
            (
                "saved display bounds the size, not the first display",
                windowed(
                    rect(2000.0, 300.0, wider_than_primary, taller_than_primary),
                    Some(SECONDARY_UUID),
                ),
                two_displays(),
                rect(0.0, 0.0, wider_than_primary, taller_than_primary),
            ),
            (
                "unknown saved display falls back to the first display",
                windowed(
                    rect(500.0, 300.0, wider_than_primary, taller_than_primary),
                    Some(UNKNOWN_UUID),
                ),
                two_displays(),
                rect(0.0, 0.0, PRIMARY_WIDTH, primary_visible_height),
            ),
            (
                "no saved display falls back to the first display",
                windowed(
                    rect(500.0, 300.0, wider_than_primary, taller_than_primary),
                    None,
                ),
                two_displays(),
                rect(0.0, 0.0, PRIMARY_WIDTH, primary_visible_height),
            ),
            (
                "fitting size keeps the saved size",
                windowed(
                    rect(500.0, 300.0, WINDOW_WIDTH, WINDOW_HEIGHT),
                    Some(PRIMARY_UUID),
                ),
                two_displays(),
                rect(0.0, 0.0, WINDOW_WIDTH, WINDOW_HEIGHT),
            ),
            (
                "no displays keeps the saved size",
                windowed(
                    rect(500.0, 300.0, wider_than_primary, taller_than_primary),
                    Some(PRIMARY_UUID),
                ),
                Vec::new(),
                rect(0.0, 0.0, wider_than_primary, taller_than_primary),
            ),
        ];
        for (name, saved, displays, expected) in cases {
            assert_eq!(
                restore_placement(&saved, &displays, false),
                Some(RestoredPlacement {
                    bounds: WindowBounds::Windowed(expected),
                    display_id: None,
                }),
                "{name}"
            );
        }
    }

    #[test]
    fn restore_rejects_non_finite_or_non_positive_geometry() {
        let valid = windowed(
            rect(100.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT),
            Some(PRIMARY_UUID),
        );
        let cases = [
            (
                "nan x",
                WindowPlacement {
                    x: f32::NAN,
                    ..valid.clone()
                },
            ),
            (
                "infinite y",
                WindowPlacement {
                    y: f32::INFINITY,
                    ..valid.clone()
                },
            ),
            (
                "nan width",
                WindowPlacement {
                    width: f32::NAN,
                    ..valid.clone()
                },
            ),
            (
                "infinite height",
                WindowPlacement {
                    height: f32::INFINITY,
                    ..valid.clone()
                },
            ),
            (
                "negative infinite width",
                WindowPlacement {
                    width: f32::NEG_INFINITY,
                    ..valid.clone()
                },
            ),
            (
                "zero width",
                WindowPlacement {
                    width: 0.0,
                    ..valid.clone()
                },
            ),
            (
                "zero height",
                WindowPlacement {
                    height: 0.0,
                    ..valid.clone()
                },
            ),
            (
                "negative width",
                WindowPlacement {
                    width: -1.0,
                    ..valid.clone()
                },
            ),
            (
                "negative height",
                WindowPlacement {
                    height: -1.0,
                    ..valid.clone()
                },
            ),
        ];
        for (name, saved) in cases {
            for origin_honored in [true, false] {
                assert_eq!(
                    restore_placement(&saved, &two_displays(), origin_honored),
                    None,
                    "{name}, origin_honored={origin_honored}"
                );
            }
        }
    }

    #[test]
    fn restore_maps_each_mode_to_its_window_bounds_variant() {
        let bounds = rect(100.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT);
        let cases = [
            (WindowMode::Windowed, WindowBounds::Windowed(bounds)),
            (WindowMode::Maximized, WindowBounds::Maximized(bounds)),
            (WindowMode::Fullscreen, WindowBounds::Fullscreen(bounds)),
        ];
        for (mode, expected) in cases {
            let saved = placement(mode, bounds, Some(PRIMARY_UUID));
            let restored =
                restore_placement(&saved, &two_displays(), true).map(|restored| restored.bounds);
            assert_eq!(restored, Some(expected), "{mode:?}");
        }
    }

    #[test]
    fn capture_then_restore_round_trips_every_window_bounds_variant() {
        let bounds = rect(2000.0, 100.0, WINDOW_WIDTH, WINDOW_HEIGHT);
        for original in [
            WindowBounds::Windowed(bounds),
            WindowBounds::Maximized(bounds),
            WindowBounds::Fullscreen(bounds),
        ] {
            let captured = WindowPlacement::capture(original, Some(SECONDARY_UUID.to_string()));
            assert_eq!(
                restore_placement(&captured, &two_displays(), true),
                Some(RestoredPlacement {
                    bounds: original,
                    display_id: Some(DisplayId::new(SECONDARY_ID)),
                }),
                "{original:?}"
            );
        }
    }

    #[test]
    fn placement_json_uses_snake_case_mode_and_round_trips() {
        let bounds = rect(-1280.0, 50.5, WINDOW_WIDTH, WINDOW_HEIGHT);
        let cases = [
            (WindowMode::Windowed, "windowed", Some(PRIMARY_UUID)),
            (WindowMode::Maximized, "maximized", None),
            (WindowMode::Fullscreen, "fullscreen", Some(SECONDARY_UUID)),
        ];
        for (mode, wire_name, display_uuid) in cases {
            let saved = placement(mode, bounds, display_uuid);
            let json = serde_json::to_value(&saved).expect("placement serializes");
            assert_eq!(json["mode"], wire_name, "{mode:?}");
            let back: WindowPlacement =
                serde_json::from_value(json).expect("placement deserializes");
            assert_eq!(back, saved, "{mode:?}");
        }
    }
}
