use super::{GLYPH_ACTION, ScriptEditorView, ScriptEntry};
use crate::navigable_list::{ListStep, NavigableList, navigable_list};
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_FIELD, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, ResizeEdge, ResizeRange, Spacing, body_family, context_menu, icon, install_resize,
    menu_divider, menu_item, mono_family, radius, scaled, spacing, status_dot, tr, with_alpha,
};
use forge_types::ScriptId;
use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, Rgba,
    SharedString, Window, div, px,
};

const STRIPE_W: Pixels = px(2.0);
const LIST_MAX_W: Pixels = px(360.0);
const LIST_MIN_W: Pixels = px(180.0);

struct ScriptsListResizeDrag;

pub(super) struct RowMenu {
    pub(super) id: ScriptId,
    pub(super) position: Point<Pixels>,
}

struct ScriptRow {
    id: ScriptId,
    child_index: usize,
}

const GROUP_HEADER_ROWS: usize = 1;
const GROUP_GAP_ROWS: usize = 1;

fn group_gap(density: Density) -> AnyElement {
    div().h(spacing(Spacing::Sm, density)).into_any_element()
}

impl NavigableList for ScriptEditorView {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        if self.rename.is_some() {
            return;
        }
        let rows = self.script_rows();
        let ids: Vec<ScriptId> = rows.iter().map(|row| row.id).collect();
        if let Some(next) = self.list_keys.step(&ids, self.selected.as_ref(), step) {
            if let Some(row) = rows.get(next) {
                self.list_scroll.scroll_to_item(row.child_index);
            }
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.rename.is_some() {
            return;
        }
        let ids: Vec<ScriptId> = self.script_rows().iter().map(|row| row.id).collect();
        if let Some(id) = self.list_keys.activation_target(&ids) {
            self.select(id, cx);
        }
    }
}

impl ScriptEditorView {
    fn open_row_menu(&mut self, id: ScriptId, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.kind_menu = None;
        self.row_menu = Some(RowMenu { id, position });
        cx.notify();
    }

    fn close_row_menu(&mut self, cx: &mut Context<Self>) {
        self.row_menu = None;
        cx.notify();
    }

    fn set_list_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let clamped = width.clamp(LIST_MIN_W, LIST_MAX_W);
        if self.list_width != clamped {
            self.list_width = clamped;
            cx.notify();
        }
    }

    fn grouped_scripts(&self) -> (Vec<&ScriptEntry>, Vec<&ScriptEntry>) {
        self.scripts
            .iter()
            .filter(|e| self.search.matches(&e.name))
            .partition(|e| e.linked.is_some())
    }

    fn script_rows(&self) -> Vec<ScriptRow> {
        let (action, standalone) = self.grouped_scripts();
        let mut rows = Vec::with_capacity(action.len() + standalone.len());
        let mut child_index = 0;
        for group in [action, standalone] {
            if group.is_empty() {
                continue;
            }
            child_index += GROUP_HEADER_ROWS;
            for entry in group {
                rows.push(ScriptRow {
                    id: entry.id,
                    child_index,
                });
                child_index += 1;
            }
            child_index += GROUP_GAP_ROWS;
        }
        rows
    }

    pub(super) fn left_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_FIELD)
                    .text_color(palette.text_primary)
                    .child(tr!("nav_script_editor")),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(self.scripts.len().to_string()),
            );

        let header = div()
            .w_full()
            .flex()
            .flex_col()
            .flex_none()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(title_row)
            .child(self.search.field().clone());

        let mut rows: Vec<AnyElement> = Vec::new();
        if self.scripts.is_empty() {
            rows.push(
                div()
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("script_editor_no_scripts"))
                    .into_any_element(),
            );
        } else {
            let (action, standalone) = self.grouped_scripts();

            if !action.is_empty() {
                rows.push(self.group_header(
                    Icon::Bolt,
                    palette.brand,
                    tr!("script_editor_group_action"),
                    action.len(),
                    palette,
                    density,
                ));
                for entry in &action {
                    rows.push(self.file_row(entry, palette.brand, palette, cx));
                }
                rows.push(group_gap(density));
            }

            if !standalone.is_empty() {
                rows.push(self.group_header(
                    Icon::FileCode,
                    palette.accent_peach,
                    tr!("script_editor_group_standalone"),
                    standalone.len(),
                    palette,
                    density,
                ));
                for entry in &standalone {
                    rows.push(self.file_row(entry, palette.accent_peach, palette, cx));
                }
                rows.push(group_gap(density));
            }
        }

        let focus = self.list_keys.focus().clone();
        let scroll = navigable_list(div().id("script-left-scroll"), &focus, cx)
            .track_scroll(&self.list_scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Xs, density))
            .children(rows);

        let pane = div()
            .flex_none()
            .w(self.list_width)
            .min_w(self.list_width)
            .max_w(self.list_width)
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(header)
            .child(scroll)
            .child(self.new_script_button(palette, density, cx));

        install_resize(
            pane,
            ScriptsListResizeDrag,
            "scripts-list-resize",
            ResizeEdge::Right,
            ResizeRange {
                min: LIST_MIN_W,
                max: LIST_MAX_W,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_list_width(*width, cx)),
        )
        .into_any_element()
    }

    fn new_script_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let brand = palette.brand;
        let hover_bg = palette.surface_overlay;
        div()
            .flex_none()
            .p(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(
                div()
                    .id("script-new")
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(spacing(Spacing::Xs, density))
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Sm, density))
                    .rounded(px(8.0))
                    .border(BORDER_THIN)
                    .border_color(palette.border_input)
                    .bg(palette.base)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(brand).bg(hover_bg))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_script(cx)))
                    .child(icon(Icon::Plus, GLYPH_ACTION, palette.brand))
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.brand)
                            .child(tr!("script_editor_new_script")),
                    ),
            )
            .into_any_element()
    }

    fn group_header(
        &self,
        group_icon: Icon,
        color: Rgba,
        label: impl Into<SharedString>,
        count: usize,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .pt(scaled(px(2.0), density))
            .px(spacing(Spacing::Xs, density))
            .pb(spacing(Spacing::Xs, density))
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(icon(group_icon, px(11.0), color))
            .child(div().child(label.into()))
            .child(div().flex_1())
            .child(
                div()
                    .text_color(palette.text_faint)
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    fn file_row(
        &self,
        entry: &ScriptEntry,
        group_color: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let id = entry.id;
        let selected = self.selected == Some(id);
        let keyboard_cursor = self.list_keys.is_at(&id);
        let renaming = self.rename.as_ref().is_some_and(|r| r.target == id);

        let icon_color = if selected {
            group_color
        } else {
            palette.text_faint
        };
        let text_color = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        let stripe = if selected {
            group_color
        } else {
            with_alpha(group_color, 0.0)
        };
        let status_color = if entry.status_ok {
            palette.success
        } else {
            palette.warning
        };
        let (subtitle, subtitle_color): (SharedString, Rgba) = match &entry.linked {
            Some(linked) => (linked.name.clone().into(), group_color),
            None => (tr!("script_editor_manual_run").into(), palette.text_faint),
        };

        let name_line: AnyElement = if renaming {
            self.rename
                .as_ref()
                .map(|r| r.editor.clone().into_any_element())
                .unwrap_or_else(|| div().into_any_element())
        } else {
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(text_color)
                .truncate()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        if event.click_count >= 2 {
                            this.start_rename(id, window, cx);
                        }
                    }),
                )
                .child(entry.name.clone())
                .into_any_element()
        };

        let middle = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(name_line)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(subtitle_color)
                    .truncate()
                    .child(subtitle),
            )
            .into_any_element();

        let trailing = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(status_color, px(6.0)));

        let mut row = div()
            .id(SharedString::from(format!("script-file-{id}")))
            .flex()
            .items_center()
            .gap(scaled(px(9.0), density))
            .py(scaled(px(7.0), density))
            .px(scaled(px(8.0), density))
            .rounded(radius(Radius::Sm))
            .border_l(STRIPE_W)
            .border_color(stripe)
            .when(selected, |d| d.bg(palette.surface_overlay))
            .when(keyboard_cursor && !selected, |d| d.bg(palette.base))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    let position = e.position;
                    this.open_row_menu(id, position, cx);
                }),
            )
            .child(icon(Icon::FileCode, px(13.0), icon_color))
            .child(middle)
            .child(trailing);

        if !renaming {
            let hover_bg = palette.base;
            row = row
                .cursor_pointer()
                .when(!selected, |d| d.hover(move |s| s.bg(hover_bg)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)));
        }

        row.into_any_element()
    }

    pub(super) fn render_row_context_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.row_menu.as_ref()?;
        let id = menu.id;
        let position = menu.position;
        let entry = self.find_entry(id)?;
        let enabled = entry.enabled;
        let view = cx.entity();

        let (toggle_label, toggle_icon) = if enabled {
            (tr!("script_editor_disable_action"), Icon::EyeOff)
        } else {
            (tr!("script_editor_enable_action"), Icon::Eye)
        };

        let items = vec![
            menu_item(
                SharedString::from(format!("script-menu-rename-{id}")),
                tr!("script_editor_rename_action"),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.start_rename(id, window, cx)
                }),
            )
            .icon(Icon::Pencil)
            .into(),
            menu_item(
                SharedString::from(format!("script-menu-enabled-{id}")),
                toggle_label,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_enabled(id, !enabled, cx)),
            )
            .icon(toggle_icon)
            .into(),
            menu_divider(),
            menu_item(
                SharedString::from(format!("script-menu-delete-{id}")),
                tr!("script_editor_delete_action"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
            )
            .icon(Icon::Eraser)
            .color(palette.danger)
            .into(),
        ];

        Some(
            context_menu(position, palette)
                .items(items)
                .dismiss_on_escape(&self.menu_focus)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_row_menu(cx));
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{InputEvent, Presentation, ThemeId};
    use forge_runtime::EventBus;
    use forge_storage::MockActionRepo;
    use forge_storage::MockScriptRepo;
    use forge_storage::{
        Language, SCRIPT_NAME_KEY, SCRIPT_RUN_NAMED_KIND, ScriptRecord, StorageError,
    };
    use forge_types::{Action, IntegrationId, ScriptContract, Variant};
    use gpui::{Entity, IntoElement, Modifiers, Render, TestAppContext, VisualTestContext};
    use std::sync::Arc;
    use time::OffsetDateTime;

    use super::super::{IntegrationAvailability, ScriptRegistry};
    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        StubEventLog, action_running, pump, runtime, step, test_backend_with_scripts_and_actions,
    };

    const STORED: [&str; 4] = ["zeta", "beta", "gamma", "alpha"];
    const LINKED: [&str; 2] = ["beta", "alpha"];

    struct NothingDisabled;

    impl IntegrationAvailability for NothingDisabled {
        fn is_disabled(&self, _: &IntegrationId) -> bool {
            false
        }
    }

    struct Hosting(Entity<ScriptEditorView>);

    impl Render for Hosting {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.0.clone())
        }
    }

    fn record(name: &str) -> ScriptRecord {
        let now = OffsetDateTime::UNIX_EPOCH;
        ScriptRecord {
            id: ScriptId::new(),
            name: name.to_owned(),
            body: "let x = 1;".to_owned(),
            contract: ScriptContract::default(),
            enabled: true,
            created_at: now,
            last_modified: now,
        }
    }

    fn linking_action(script: &str) -> Action {
        let mut running = step(SCRIPT_RUN_NAMED_KIND, true);
        running.config.insert(
            SCRIPT_NAME_KEY.to_owned(),
            Variant::String(script.to_owned()),
        );
        action_running(&format!("runs {script}"), vec![running])
    }

    struct Rig {
        rt: tokio::runtime::Runtime,
        view: Entity<ScriptEditorView>,
        records: Vec<ScriptRecord>,
        vcx: VisualTestContext,
    }

    impl Rig {
        fn mount(cx: &mut TestAppContext) -> Self {
            install_language(Language::En);
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
                crate::actions::register_key_bindings(cx);
            });
            let rt = runtime();
            let records: Vec<ScriptRecord> = STORED.iter().map(|name| record(name)).collect();

            let mut scripts = MockScriptRepo::new();
            let listed = records.clone();
            scripts.expect_list().returning(move || Ok(listed.clone()));
            let found = records.clone();
            scripts
                .expect_get()
                .returning(move |id| Ok(found.iter().find(|r| r.id == id).cloned()));
            scripts.expect_telemetry().returning(|_| {
                Err(StorageError::NotFound {
                    key: "telemetry".to_owned(),
                })
            });
            let mut actions = MockActionRepo::new();
            let linking: Vec<Action> = LINKED.iter().map(|name| linking_action(name)).collect();
            actions.expect_list().returning(move || Ok(linking.clone()));

            let backend =
                test_backend_with_scripts_and_actions(Arc::new(scripts), Arc::new(actions));
            let bus = {
                let _entered = rt.enter();
                EventBus::new(Arc::new(StubEventLog))
            };
            let handle = rt.handle().clone();
            let view = cx.new(|cx| {
                ScriptEditorView::new(
                    backend,
                    Arc::new(ScriptRegistry::new()),
                    bus,
                    Arc::new(NothingDisabled),
                    handle,
                    cx,
                )
            });
            let hosted = view.clone();
            let window = cx.add_window(|_, _| Hosting(hosted));
            let vcx = VisualTestContext::from_window(window.into(), cx);
            let mut rig = Self {
                rt,
                view,
                records,
                vcx,
            };
            rig.settle();
            rig
        }

        fn settle(&mut self) {
            for _ in 0..6 {
                pump(&self.rt);
                self.vcx.run_until_parked();
            }
        }

        fn id_of(&self, name: &str) -> ScriptId {
            self.records.iter().find(|r| r.name == name).unwrap().id
        }

        fn name_of(&self, id: ScriptId) -> String {
            let found = self.records.iter().find(|r| r.id == id);
            found.unwrap().name.clone()
        }

        fn search(&mut self, query: &str) {
            let changed = InputEvent::Changed(query.to_owned().into());
            self.view.update(&mut self.vcx, |view, cx| {
                view.search.on_changed(&changed);
                cx.notify();
            });
            self.vcx.run_until_parked();
        }

        fn press(&mut self, keys: &str) {
            self.vcx.simulate_keystrokes(keys);
            self.settle();
        }

        fn row_count(&mut self) -> usize {
            self.view
                .read_with(&self.vcx, |view, _| view.script_rows().len())
        }

        fn climb_to_the_top(&mut self) {
            for _ in 0..STORED.len() {
                self.press("up");
            }
        }

        fn cursor(&mut self) -> Option<String> {
            let names = STORED.map(|name| (self.id_of(name), name));
            self.view.read_with(&self.vcx, |view, _| {
                names
                    .iter()
                    .find(|(id, _)| view.list_keys.is_at(id))
                    .map(|(_, name)| (*name).to_owned())
            })
        }

        fn selected(&mut self) -> Option<String> {
            let selected = self.view.read_with(&self.vcx, |view, _| view.selected);
            selected.map(|id| self.name_of(id))
        }

        fn opened(&mut self) -> Option<String> {
            let open = self
                .view
                .read_with(&self.vcx, |view, _| view.open.as_ref().map(|open| open.id));
            open.map(|id| self.name_of(id))
        }

        fn keyboard_walk(&mut self) -> Vec<String> {
            let rows = self.row_count();
            self.climb_to_the_top();
            let mut walked = vec![self.cursor().unwrap()];
            for _ in 1..rows {
                self.press("down");
                walked.push(self.cursor().unwrap());
            }
            walked
        }

        fn click_row(&mut self, position: usize) {
            let bounds = self.view.read_with(&self.vcx, |view, _| {
                let child = view.script_rows()[position].child_index;
                view.list_scroll.bounds_for_item(child).unwrap()
            });
            self.vcx.simulate_click(bounds.center(), Modifiers::none());
            self.settle();
        }

        fn clicked_walk(&mut self) -> Vec<String> {
            (0..self.row_count())
                .map(|position| {
                    self.click_row(position);
                    self.selected().unwrap()
                })
                .collect()
        }
    }

    #[gpui::test]
    fn arrows_walk_the_script_list_in_the_order_it_renders_under_any_search(
        cx: &mut TestAppContext,
    ) {
        for (query, expected) in [
            ("", vec!["beta", "alpha", "zeta", "gamma"]),
            ("et", vec!["beta", "zeta"]),
            ("alpha", vec!["alpha"]),
            ("gamma", vec!["gamma"]),
        ] {
            let mut rig = Rig::mount(cx);
            rig.search(query);

            assert_eq!(rig.keyboard_walk(), expected, "keys under {query:?}");
            assert_eq!(rig.clicked_walk(), expected, "rendered under {query:?}");
        }
    }

    #[gpui::test]
    fn enter_on_the_cursor_row_opens_the_script_a_click_on_that_row_opens(cx: &mut TestAppContext) {
        let mut by_key = Rig::mount(cx);
        by_key.climb_to_the_top();
        by_key.press("down enter");

        let mut by_mouse = Rig::mount(cx);
        by_mouse.click_row(1);

        assert_eq!(by_key.opened().as_deref(), Some("alpha"));
        assert_eq!(by_mouse.opened().as_deref(), Some("alpha"));
    }

    #[gpui::test]
    fn a_search_that_hides_the_cursor_row_takes_the_cursor_with_it(cx: &mut TestAppContext) {
        let mut rig = Rig::mount(cx);
        rig.climb_to_the_top();
        rig.press("down");
        assert_eq!(rig.cursor().as_deref(), Some("alpha"));

        rig.search("gamma");
        rig.press("enter");
        assert_eq!(rig.selected().as_deref(), Some("zeta"));

        rig.press("down");
        assert_eq!(rig.cursor().as_deref(), Some("gamma"));
    }

    #[gpui::test]
    fn an_inline_rename_silences_the_list_keys(cx: &mut TestAppContext) {
        let mut rig = Rig::mount(cx);
        rig.climb_to_the_top();
        let renamed = rig.id_of("gamma");
        rig.view.update_in(&mut rig.vcx, |view, window, cx| {
            view.start_rename(renamed, window, cx);
            view.step_list(ListStep::Next, cx);
            view.activate_list(window, cx);
        });
        rig.settle();

        assert_eq!(rig.cursor().as_deref(), Some("beta"));
        assert_eq!(rig.selected().as_deref(), Some("zeta"));
    }
}
