use std::collections::BTreeMap;

use forge_components::{ActivePresentation, InputEvent, PickerEvent, tr};
use forge_platform_core::{
    CollectionField, CollectionItem, CollectionItemId, QuickActionFieldValue,
};
use gpui::{App, Context, EventEmitter, Window, div, prelude::*, px};

use crate::integrations::quick_action_controls::{
    OptionListSupport, QuickFieldControls, QuickFieldHandlers, QuickFieldIds, QuickFieldSeed,
};

const FORM_GAP: gpui::Pixels = px(12.0);
const FIELD_IDS: QuickFieldIds = QuickFieldIds {
    toggle: "collection-form-toggle",
    choice: "collection-form-choice",
    spinner: "collection-form-spinner",
    retry: "collection-form-retry",
};

pub(crate) enum CollectionFormEvent {
    Cancelled,
}

pub(crate) struct CollectionForm {
    editing: Option<CollectionItemId>,
    editing_title: Option<String>,
    controls: QuickFieldControls,
    submitting: bool,
}

impl EventEmitter<CollectionFormEvent> for CollectionForm {}

impl CollectionForm {
    pub(crate) fn new(
        schema: &[CollectionField],
        item: Option<&CollectionItem>,
        cx: &mut Context<Self>,
    ) -> Self {
        let seeds = schema
            .iter()
            .map(|declared| QuickFieldSeed {
                spec: &declared.field,
                initial: item
                    .and_then(|item| item.values.get(&declared.field.key))
                    .or(declared.field.default.as_ref()),
                max_chars: declared.max_chars,
            })
            .collect();
        let lists = OptionListSupport {
            declared: &[],
            unavailable: tr!("collection_field_unsupported"),
        };
        Self {
            editing: item.map(|item| item.id.clone()),
            editing_title: item.map(|item| item.title.clone()),
            controls: QuickFieldControls::new(seeds, &lists, Self::on_field_event, cx),
            submitting: false,
        }
    }

    pub(crate) fn editing(&self) -> Option<&CollectionItemId> {
        self.editing.as_ref()
    }

    pub(crate) fn editing_title(&self) -> Option<&str> {
        self.editing_title.as_deref()
    }

    pub(crate) fn is_submitting(&self) -> bool {
        self.submitting
    }

    pub(crate) fn set_submitting(&mut self, submitting: bool, cx: &mut Context<Self>) {
        self.submitting = submitting;
        cx.notify();
    }

    pub(crate) fn show_field_error(
        &mut self,
        key: &str,
        message: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let fields = &mut self.controls.fields;
        let Some(index) = fields.iter().position(|field| field.key == key) else {
            return false;
        };
        if let Some(field) = fields.get_mut(index) {
            field.error = Some(message);
        }
        self.controls.revalidate(index, cx);
        cx.notify();
        true
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.controls.focus_first(window, cx);
    }

    pub(crate) fn can_submit(&self, cx: &App) -> bool {
        !self.submitting
            && self
                .controls
                .fields
                .iter()
                .all(|field| !field.entry_invalid(cx))
    }

    pub(crate) fn values(&self, cx: &App) -> BTreeMap<String, QuickActionFieldValue> {
        self.controls.values(cx)
    }

    fn on_field_event(&mut self, index: usize, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::Cancelled => cx.emit(CollectionFormEvent::Cancelled),
            InputEvent::Changed(_) => {
                self.controls.entry_changed(index, cx);
                cx.notify();
            }
            InputEvent::Submitted(_) | InputEvent::Blurred(_) => {}
        }
    }

    fn toggle_field(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.controls.toggle(index) {
            cx.notify();
        }
    }

    fn open_choice(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.controls
            .open_choice_picker(index, Self::on_choice_picker, window, cx);
        cx.notify();
    }

    fn on_choice_picker(&mut self, index: usize, event: &PickerEvent, cx: &mut Context<Self>) {
        match event {
            PickerEvent::Selected(id) => {
                self.controls.pick(index, id);
                cx.notify();
            }
            PickerEvent::Cancelled => self.close_choice(cx),
        }
    }

    fn close_choice(&mut self, cx: &mut Context<Self>) {
        self.controls.open_choice = None;
        cx.notify();
    }
}

impl Render for CollectionForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let handlers = QuickFieldHandlers {
            toggle: Self::toggle_field,
            open: Self::open_choice,
            close: Self::close_choice,
            retry: None,
        };
        let mut body = div().w_full().flex().flex_col().gap(FORM_GAP);
        for index in 0..self.controls.fields.len() {
            body = body.child(
                self.controls
                    .render_field(index, FIELD_IDS, &handlers, &palette, cx),
            );
        }
        body
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{Density, ThemeId};
    use forge_platform_core::{CollectionItemAccess, QuickActionField, QuickActionFieldKind};
    use gpui::{Entity, TestAppContext};

    use super::*;
    use crate::integrations::quick_action_controls::FieldControl;
    use forge_components::Presentation;

    const TITLE: &str = "title";
    const COST: &str = "cost";
    const INPUT: &str = "needs_input";
    const TITLE_LIMIT: usize = 45;

    fn field(key: &str, kind: QuickActionFieldKind, required: bool) -> QuickActionField {
        QuickActionField {
            key: key.to_owned(),
            label: forge_types::VerbatimText::new(key).into(),
            kind,
            default: None,
            placeholder: None,
            hint: None,
            required,
        }
    }

    fn schema() -> Vec<CollectionField> {
        vec![
            CollectionField {
                field: field(TITLE, QuickActionFieldKind::Text, true),
                max_chars: Some(TITLE_LIMIT),
            },
            CollectionField {
                field: field(
                    COST,
                    QuickActionFieldKind::Int {
                        min: 1,
                        max: i64::MAX,
                    },
                    true,
                ),
                max_chars: None,
            },
            CollectionField {
                field: field(INPUT, QuickActionFieldKind::Toggle, false),
                max_chars: None,
            },
        ]
    }

    fn existing() -> CollectionItem {
        CollectionItem {
            id: CollectionItemId::new("rw1"),
            title: "Hydrate".to_owned(),
            values: BTreeMap::from([
                (
                    TITLE.to_owned(),
                    QuickActionFieldValue::Text("Hydrate".to_owned()),
                ),
                (COST.to_owned(), QuickActionFieldValue::Int(250)),
                (INPUT.to_owned(), QuickActionFieldValue::Toggle(true)),
            ]),
            toggles: BTreeMap::new(),
            access: CollectionItemAccess::Manageable,
        }
    }

    fn open(cx: &mut TestAppContext, item: Option<CollectionItem>) -> Entity<CollectionForm> {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.new(|cx| CollectionForm::new(&schema(), item.as_ref(), cx))
        })
    }

    fn type_title(cx: &mut TestAppContext, form: &Entity<CollectionForm>, title: String) {
        cx.update(|cx| {
            let input = match &form.read(cx).controls.fields[0].control {
                FieldControl::Text(input) => input.clone(),
                _ => unreachable!("the first declared field is the text title"),
            };
            input.update(cx, |input, cx| input.set_content(title, cx));
        });
    }

    fn can_submit(cx: &mut TestAppContext, form: &Entity<CollectionForm>) -> bool {
        cx.update(|cx| form.read(cx).can_submit(cx))
    }

    #[gpui::test]
    fn editing_an_item_prefills_every_declared_value(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        let values = cx.update(|cx| form.read(cx).values(cx));

        assert_eq!(values, existing().values);
    }

    #[gpui::test]
    fn the_title_character_limit_blocks_submit_only_past_the_limit(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));
        let mut verdicts = Vec::new();

        for title in [
            "a".repeat(TITLE_LIMIT),
            "ї".repeat(TITLE_LIMIT),
            "a".repeat(TITLE_LIMIT + 1),
        ] {
            type_title(cx, &form, title);
            verdicts.push(can_submit(cx, &form));
        }

        assert_eq!(verdicts, vec![true, true, false]);
    }

    #[gpui::test]
    fn a_blank_required_title_blocks_submit(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        type_title(cx, &form, "   ".to_owned());

        assert!(!can_submit(cx, &form));
    }

    #[gpui::test]
    fn a_new_form_starts_blocked_until_the_required_title_is_typed(cx: &mut TestAppContext) {
        let form = open(cx, None);
        let before = can_submit(cx, &form);

        type_title(cx, &form, "Hydrate".to_owned());

        assert_eq!((before, can_submit(cx, &form)), (false, true));
    }

    #[gpui::test]
    fn a_server_error_lands_only_on_a_declared_field(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        let shown = cx.update(|cx| {
            form.update(cx, |form, cx| {
                (
                    form.show_field_error(TITLE, "taken".to_owned(), cx),
                    form.show_field_error("in_stock", "nope".to_owned(), cx),
                )
            })
        });

        assert_eq!(shown, (true, false));
    }

    #[gpui::test]
    fn submitting_blocks_a_second_submit(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        cx.update(|cx| form.update(cx, |form, cx| form.set_submitting(true, cx)));

        assert!(!can_submit(cx, &form));
    }

    #[gpui::test]
    fn typed_reward_labels_render_in_ukrainian(cx: &mut TestAppContext) {
        crate::app::i18n::install_language(forge_storage::Language::Uk);
        let code = |name| forge_types::MessageCode::from_static("twitch", name);
        let schema = vec![
            CollectionField {
                field: QuickActionField {
                    label: code("label_title").into(),
                    ..field(TITLE, QuickActionFieldKind::Text, true)
                },
                max_chars: None,
            },
            CollectionField {
                field: QuickActionField {
                    label: code("label_require_viewer_input").into(),
                    ..field(INPUT, QuickActionFieldKind::Toggle, false)
                },
                max_chars: None,
            },
        ];
        cx.update(|cx| cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy)));
        let form = cx.update(|cx| cx.new(|cx| CollectionForm::new(&schema, None, cx)));

        let labels = cx.update(|cx| {
            form.read(cx)
                .controls
                .fields
                .iter()
                .map(|field| field.label.clone())
                .collect::<Vec<_>>()
        });

        assert_eq!(labels, ["Назва", "Вимагати введення від глядача"]);
    }
}
