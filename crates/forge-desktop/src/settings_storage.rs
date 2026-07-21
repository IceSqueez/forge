use std::sync::Arc;

use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, ForgePalette, Icon,
    InputEvent, Spacing, TextInput, card, field_hint, field_title, icon, primary_button, spacing,
    tr,
};
use forge_storage::{
    DataProvider, SettingsRepo, chat_history_display_limit, chat_history_store_limit,
    set_chat_history_display_limit, set_chat_history_store_limit,
};
use gpui::{
    ClickEvent, Context, Entity, FontWeight, SharedString, Subscription, Window, div, prelude::*,
    px,
};

use crate::presentation::ActivePresentation;

const DEFAULT_STORE_LIMIT: u32 = 5000;
const DEFAULT_DISPLAY_LIMIT: u32 = 500;

pub struct SettingsStorageView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,

    store_limit: u32,
    display_limit: u32,

    store_input: Entity<TextInput>,
    display_input: Entity<TextInput>,
    _subs: Vec<Subscription>,
}

impl SettingsStorageView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let store_input = cx.new(|cx| {
            TextInput::new(DEFAULT_STORE_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let display_input = cx.new(|cx| {
            TextInput::new(DEFAULT_DISPLAY_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });

        let subs = vec![
            cx.subscribe(&store_input, |this, _input, event: &InputEvent, cx| {
                if let InputEvent::Submitted(_) = event {
                    this.commit_store(cx);
                }
            }),
            cx.subscribe(&display_input, |this, _input, event: &InputEvent, cx| {
                if let InputEvent::Submitted(_) = event {
                    this.commit_display(cx);
                }
            }),
        ];

        let mut view = Self {
            backend,
            rt_handle,
            store_limit: DEFAULT_STORE_LIMIT,
            display_limit: DEFAULT_DISPLAY_LIMIT,
            store_input,
            display_input,
            _subs: subs,
        };
        view.load(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(load_limits(repo).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_loaded(result, cx));
            }
        })
        .detach();
    }

    fn apply_loaded(&mut self, result: Result<(u32, u32), String>, cx: &mut Context<Self>) {
        match result {
            Ok((store, display)) => {
                self.store_limit = store;
                self.display_limit = display;
                self.store_input
                    .update(cx, |i, cx| i.set_content(store.to_string(), cx));
                self.display_input
                    .update(cx, |i, cx| i.set_content(display.to_string(), cx));
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load chat history limits");
            }
        }
        cx.notify();
    }

    fn commit_store(&mut self, cx: &mut Context<Self>) {
        match parse_limit(self.store_input.read(cx).content()) {
            Some(value) => {
                self.store_limit = value;
                self.store_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.rt_handle.spawn(async move {
                    if let Err(e) = set_chat_history_store_limit(repo.as_ref(), value).await {
                        tracing::warn!(error = %e, "failed to persist chat history keep limit");
                    }
                });
            }
            None => {
                let restore = self.store_limit.to_string();
                self.store_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn commit_display(&mut self, cx: &mut Context<Self>) {
        match parse_limit(self.display_input.read(cx).content()) {
            Some(value) => {
                self.display_limit = value;
                self.display_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.rt_handle.spawn(async move {
                    if let Err(e) = set_chat_history_display_limit(repo.as_ref(), value).await {
                        tracing::warn!(error = %e, "failed to persist chat history display limit");
                    }
                });
            }
            None => {
                let restore = self.display_limit.to_string();
                self.display_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn backup_db(&self) {
        let backend = Arc::clone(&self.backend);
        self.rt_handle.spawn(async move {
            let stamp = time::OffsetDateTime::now_utc().unix_timestamp();
            let path =
                forge_platform_core::paths::data_dir().join(format!("forge-backup-{stamp}.db"));
            match backend.export(&path).await {
                Ok(()) => tracing::info!(path = %path.display(), "DB backup created"),
                Err(e) => tracing::warn!(error = %e, "DB backup failed"),
            }
        });
    }

    fn limit_field(
        &self,
        label: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        input: Entity<TextInput>,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(label, palette))
            .child(field_hint(hint, palette))
            .child(div().max_w(px(200.0)).child(input))
    }
}

impl Render for SettingsStorageView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let db_path = forge_platform_core::paths::data_dir().join("forge.db");
        let backup_btn = primary_button(tr!("settings_storage_backup_btn"), &palette).on_click(
            "settings-db-backup",
            cx.listener(|this, _: &ClickEvent, _, _| this.backup_db()),
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(info_row(
                tr!("settings_storage_db_path_label"),
                db_path.display().to_string(),
                &palette,
            ))
            .child(backup_btn)
            .child(field_hint(tr!("settings_storage_backup_hint"), &palette))
            .child(self.limit_field(
                tr!("settings_storage_keep_limit_label"),
                tr!("settings_storage_keep_limit_hint"),
                self.store_input.clone(),
                &palette,
                density,
            ))
            .child(self.limit_field(
                tr!("settings_storage_display_limit_label"),
                tr!("settings_storage_display_limit_hint"),
                self.display_input.clone(),
                &palette,
                density,
            ));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::Folder,
                tr!("settings_storage_section_title"),
                &palette,
            ))
            .child(card(body, &palette))
    }
}

async fn load_limits(repo: Arc<dyn SettingsRepo>) -> Result<(u32, u32), String> {
    let store = chat_history_store_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let display = chat_history_display_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok((store, display))
}

fn parse_limit(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|v| *v >= 1)
}

fn pane_header(
    glyph: Icon,
    title: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .child(icon(glyph, px(18.0), palette.brand))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(title.into()),
        )
}

fn info_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xs, Density::Cozy))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(value.into()),
        )
}

#[cfg(test)]
mod tests {
    use super::parse_limit;

    #[test]
    fn parse_limit_accepts_positive_integers_and_rejects_everything_else() {
        // Whitespace is trimmed; zero is rejected by the `>= 1` floor alongside
        // empty, non-numeric, negative, and fractional input.
        for (raw, expected) in [
            ("500", Some(500)),
            (" 42 ", Some(42)),
            ("1", Some(1)),
            ("0", None),
            ("", None),
            ("abc", None),
            ("-5", None),
            ("1.5", None),
        ] {
            assert_eq!(parse_limit(raw), expected, "parse_limit({raw:?})");
        }
    }
}
