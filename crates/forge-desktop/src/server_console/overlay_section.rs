use super::credentials::mono_field;
use super::{
    CONTROL_GLYPH, HEADER_GLYPH, LINK_GLYPH, OwnedOverlayEntry, OwnedOverlayKind,
    ServerConsoleView, caption,
};
use crate::async_bridge::{self, ErrorSink};
use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XXS, ForgePalette, Icon, KeyActivated, Radius, Spacing,
    body_family, card, empty_state, fmt_bytes, icon, mono_family, radius, spacing, tooltip_builder,
    tr,
};
use forge_overlay::MARKUP_FILE;
use forge_server::{overlay_file_url, overlay_origin, overlay_page_url};
use gpui::{AnyElement, ClickEvent, Context, ElementId, FontWeight, Pixels, div, prelude::*, px};
use std::ffi::OsStr;

const FILE_GLYPH: Pixels = px(12.0);

pub(super) struct OverlayListing {
    pub(super) root: String,
    pub(super) entries: Vec<OwnedOverlayEntry>,
}

pub(super) fn is_html_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"))
}

pub(super) async fn scan_overlay_root(root: &std::path::Path) -> OverlayListing {
    let root_str = root.to_string_lossy().into_owned();
    let mut read_dir = match tokio::fs::read_dir(root).await {
        Ok(read_dir) => read_dir,
        Err(_) => {
            return OverlayListing {
                root: root_str,
                entries: Vec::new(),
            };
        }
    };

    let mut entries: Vec<OwnedOverlayEntry> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = match entry.metadata().await {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if meta.is_dir() {
            let mut child_count: u32 = 0;
            let mut overlay = false;
            if let Ok(mut child) = tokio::fs::read_dir(entry.path()).await {
                while let Ok(Some(child_entry)) = child.next_entry().await {
                    child_count = child_count.saturating_add(1);
                    overlay |= child_entry.file_name().as_os_str() == OsStr::new(MARKUP_FILE);
                }
            }
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::Dir { overlay },
                size_bytes: 0,
                child_count,
            });
        } else {
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::File {
                    html: is_html_path(&entry.path()),
                },
                size_bytes: meta.len(),
                child_count: 0,
            });
        }
    }

    entries.sort_by(|a, b| {
        let dir_a = matches!(a.kind, OwnedOverlayKind::Dir { .. });
        let dir_b = matches!(b.kind, OwnedOverlayKind::Dir { .. });
        match (dir_a, dir_b) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase()),
        }
    });

    OverlayListing {
        root: root_str,
        entries,
    }
}

impl ServerConsoleView {
    fn browser_source_entry(&self) -> Option<&OwnedOverlayEntry> {
        self.selected_overlay_entry
            .as_deref()
            .and_then(|name| self.overlay_entries.iter().find(|entry| entry.name == name))
            .filter(|entry| entry.is_selectable())
            .or_else(|| self.overlay_entries.iter().find(|e| e.hosts_overlay()))
            .or_else(|| self.overlay_entries.iter().find(|e| e.is_html()))
    }

    pub(super) fn browser_source_url(&self) -> Option<String> {
        self.browser_source_url_with(self.overlay_token.as_ref()?.expose())
    }

    pub(super) fn displayed_browser_source_url(&self) -> Option<String> {
        self.browser_source_url_with(&crate::overlay_link_token::displayed(
            self.overlay_token.as_ref()?.expose(),
        ))
    }

    fn browser_source_url_with(&self, token: &str) -> Option<String> {
        if !self.running {
            return None;
        }
        let origin = overlay_origin(self.bind_address.as_deref()?, self.routable_host.as_deref());
        let entry = self.browser_source_entry()?;
        Some(match entry.kind {
            OwnedOverlayKind::Dir { .. } => overlay_page_url(&origin, token, &entry.name),
            OwnedOverlayKind::File { .. } => overlay_file_url(&origin, token, &entry.name),
        })
    }

    pub(super) fn copy_browser_source_url(&mut self, cx: &mut Context<Self>) {
        if let Some(url) = self.browser_source_url() {
            crate::toasts::copy_to_clipboard(url, cx);
        }
    }

    fn open_overlay_folder(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                let root = handle.overlay_root().await;
                async_bridge::open_path((*root).clone()).await
            },
            ErrorSink::Toast,
            tr!("server_open_overlay_folder_failed"),
            cx,
        );
    }

    pub(super) fn refresh_overlay_listing(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let root = handle.overlay_root().await;
                scan_overlay_root(root.as_ref()).await
            },
            |this, listing, cx| {
                this.apply_overlay_listing(listing);
                cx.notify();
            },
            cx,
        );
    }

    pub(super) fn apply_overlay_listing(&mut self, overlay: OverlayListing) {
        self.overlay_root = overlay.root;
        self.overlay_entries = overlay.entries;
        if let Some(name) = &self.selected_overlay_entry
            && !self.overlay_entries.iter().any(|entry| &entry.name == name)
        {
            self.selected_overlay_entry = None;
        }
    }

    pub(super) fn select_overlay_entry(&mut self, name: String, cx: &mut Context<Self>) {
        self.selected_overlay_entry = Some(name);
        cx.notify();
    }

    pub(super) fn overlay_panel(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::Folder, HEADER_GLYPH, palette.warning))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("server_overlay_host_title")),
            );

        let root_label = if self.overlay_root.is_empty() {
            "-".to_owned()
        } else {
            self.overlay_root.clone()
        };
        let path_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                mono_field(palette, density).flex_1().child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(FONT_XXS)
                        .text_color(palette.text_primary)
                        .child(root_label),
                ),
            )
            .child(
                div()
                    .id("srv-open-folder")
                    .flex()
                    .flex_none()
                    .items_center()
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(palette.border_input))
                    .tooltip(tooltip_builder(tr!("server_open_overlay_folder"), palette))
                    .key_activated(palette.text_primary)
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_overlay_folder(cx)),
                    )
                    .child(icon(Icon::FolderOpen, CONTROL_GLYPH, palette.text_muted)),
            )
            .child(
                div()
                    .id("srv-refresh-overlay-files")
                    .flex()
                    .flex_none()
                    .items_center()
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(palette.border_input))
                    .tooltip(tooltip_builder(
                        tr!("server_refresh_overlay_files"),
                        palette,
                    ))
                    .key_activated(palette.text_primary)
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_overlay_listing(cx)),
                    )
                    .child(icon(Icon::Refresh, CONTROL_GLYPH, palette.text_muted)),
            );

        let files: AnyElement = if self.overlay_entries.is_empty() {
            empty_state(tr!("server_overlay_files_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let mut col = div().w_full().flex().flex_col();
            for entry in &self.overlay_entries {
                col = col.child(self.overlay_entry_row(entry, palette, density, cx));
            }
            col.into_any_element()
        };

        let body = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Sm, density))
            .child(path_row)
            .child(files)
            .child(self.browser_source_block(palette, density, cx));

        let inner = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(body);

        card(inner, palette)
            .padding(px(0.0))
            .radius(Radius::Md)
            .full_width()
            .into_any_element()
    }

    fn overlay_entry_row(
        &self,
        entry: &OwnedOverlayEntry,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (glyph, glyph_color, size_label) = match entry.kind {
            OwnedOverlayKind::Dir { .. } => (
                Icon::Folder,
                palette.warning,
                tr!("server_overlay_dir_files", count = entry.child_count as i64),
            ),
            OwnedOverlayKind::File { .. } => {
                (Icon::FileCode, palette.info, fmt_bytes(entry.size_bytes))
            }
        };
        let selected = self
            .browser_source_entry()
            .is_some_and(|current| current.name == entry.name);
        let name_color = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };

        let mut row = div()
            .id((ElementId::from("srv-overlay-entry"), entry.name.clone()))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .child(icon(glyph, FILE_GLYPH, glyph_color))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(name_color)
                    .child(entry.name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(size_label),
            );

        if entry.is_selectable() {
            let name = entry.name.clone();
            row = row
                .cursor_pointer()
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.select_overlay_entry(name.clone(), cx)
                }));
        }

        row.into_any_element()
    }

    fn browser_source_block(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url = self.displayed_browser_source_url();

        let value = match url.clone() {
            Some(url) => div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_color(palette.info)
                .child(url),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .text_color(palette.text_faint)
                .child("-"),
        };

        let mut field = mono_field(palette, density)
            .text_size(FONT_XXS)
            .child(value);
        if url.is_some() {
            field = field.child(
                div()
                    .id("srv-overlay-url-copy")
                    .flex()
                    .flex_none()
                    .cursor_pointer()
                    .tooltip(tooltip_builder(tr!("common_copy"), palette))
                    .key_activated(palette.text_primary)
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.copy_browser_source_url(cx)),
                    )
                    .child(icon(Icon::Copy, LINK_GLYPH, palette.brand)),
            );
        }

        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .pt(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(caption(tr!("server_overlay_browser_source_url"), palette))
            .child(field)
            .into_any_element()
    }
}
