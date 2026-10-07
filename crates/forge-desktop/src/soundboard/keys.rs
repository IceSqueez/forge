use forge_components::{BORDER_THIN, ForgePalette, Icon, icon, mono_family, tooltip_builder, tr};
use forge_types::ClipId;
use gpui::{AnyElement, Context, Pixels, div, prelude::*, px};

use super::{HOTKEY_FS, SoundboardView};
use crate::async_bridge;
use crate::clip_key_state::{PadKey, canonical_combo};
use crate::hotkey_sync::HotkeyReconciler;

const HOTKEY_RADIUS: Pixels = px(4.0);
const HOTKEY_PAD_X: Pixels = px(6.0);
const HOTKEY_PAD_Y: Pixels = px(1.0);
const HOTKEY_WARN_GAP: Pixels = px(3.0);
const HOTKEY_WARN_GLYPH: Pixels = px(10.0);

impl SoundboardView {
    pub(super) fn reload_key_holders(&self, cx: &mut Context<Self>) {
        let Some(load) = self.keys.load_rows() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            load,
            |this, result, cx| match result {
                Ok(rows) => {
                    this.keys.set_rows(rows);
                    let holders = this.keys.holders();
                    if let Some(modal) = this.modal.as_ref() {
                        modal.update(cx, |editor, cx| editor.set_holders(holders, cx));
                    }
                    cx.notify();
                }
                Err(message) => {
                    tracing::warn!(error = %message, "hotkey bindings unavailable for clip key checks");
                }
            },
            cx,
        );
    }

    pub(super) fn render_hotkey_badge(
        &self,
        index: usize,
        id: ClipId,
        hotkey: &str,
        palette: &ForgePalette,
    ) -> AnyElement {
        let combo = canonical_combo(hotkey);
        let state = self.keys.pad_key(id, &combo);
        let (text, border) = if state.is_warning() {
            (palette.warning, palette.warning)
        } else {
            (palette.text_secondary, palette.surface_overlay)
        };
        let badge = div()
            .id(("sb-pad-key", index))
            .flex()
            .items_center()
            .gap(HOTKEY_WARN_GAP)
            .font_family(mono_family())
            .text_size(HOTKEY_FS)
            .text_color(text)
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(border)
            .rounded(HOTKEY_RADIUS)
            .px(HOTKEY_PAD_X)
            .py(HOTKEY_PAD_Y);
        let reason = match &state {
            PadKey::Live => return badge.child(combo).into_any_element(),
            PadKey::NotLive => tr!("soundboard_pad_key_not_live", combo = combo.as_str()),
            PadKey::EngineUnavailable => {
                tr!(
                    "soundboard_pad_key_engine_unavailable",
                    combo = combo.as_str()
                )
            }
            PadKey::Shared(holder) => tr!(
                "soundboard_pad_key_shared",
                combo = combo.as_str(),
                holder = holder.label()
            ),
        };
        badge
            .tooltip(tooltip_builder(reason, palette))
            .child(icon(
                Icon::AlertTriangle,
                HOTKEY_WARN_GLYPH,
                palette.warning,
            ))
            .child(combo)
            .into_any_element()
    }
}

pub(super) fn unregistered_combo(
    hotkey: Option<&str>,
    reconciler: Option<&HotkeyReconciler>,
) -> Option<String> {
    let reconciler = reconciler?;
    let combo = canonical_combo(hotkey?);
    let live = reconciler
        .client()
        .registered_combos()
        .iter()
        .any(|(_, registered)| registered.as_str() == combo);
    (!live).then_some(combo)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use super::*;

    struct SilentPublisher;

    impl forge_events::EventPublisher for SilentPublisher {
        fn publish(&self, _: forge_events::Event) {}
    }

    #[tokio::test]
    async fn the_save_warning_names_the_clip_key_only_when_the_os_did_not_take_it() {
        let backend = crate::test_support::sandboxed_backend("sqlite::memory:", [0x11; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn forge_storage::DataProvider>);
        let (client, _recorder) = forge_hotkey::testing::test_client(
            forge_hotkey::HotkeyConfig::default(),
            Arc::new(SilentPublisher),
        );
        let reconciler = HotkeyReconciler::new(
            Arc::clone(&client),
            backend.trigger_instance_repo(),
            backend.soundboard_clips_repo(),
        );
        client
            .register(forge_hotkey::HotkeyCombo::parse("F9").unwrap())
            .await
            .unwrap();

        for (case, hotkey, engine, expected) in [
            ("registered key", Some("F9"), Some(&*reconciler), None),
            (
                "hand-typed registered key",
                Some("f9"),
                Some(&*reconciler),
                None,
            ),
            (
                "refused key",
                Some("f10"),
                Some(&*reconciler),
                Some("F10".to_owned()),
            ),
            ("no key", None, Some(&*reconciler), None),
            ("no hotkey engine", Some("F10"), None, None),
        ] {
            assert_eq!(unregistered_combo(hotkey, engine), expected, "{case}");
        }
    }
}
