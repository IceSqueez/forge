use super::field::SubFormField;
use super::*;
use crate::globals::variant_kind_color;
use forge_components::{
    VariableEntry, VariableFormat, VariableGroup, VariablePicker, VariablePickerEvent,
    VariablePickerLabels, dropdown, ghost_button_with_icon,
};
use forge_registry::{ScopeOrigin, ScopedVariable};
use forge_types::{
    DatePattern, NamesLanguage, PatternZone, VariantKind, current_names_language, datetime_to_text,
};
use time::OffsetDateTime;

const UK_DATETIME_PRESETS: [&str; 4] = ["d MMMM, HH:mm", "HH:mm", "dd.MM.yyyy", "d MMMM"];
const EN_DATETIME_PRESETS: [&str; 4] = ["MMMM d, HH:mm", "HH:mm", "MM/dd/yyyy", "d MMMM"];

#[derive(Clone)]
pub(super) enum InsertTarget {
    Input(Entity<TextInput>),
    Area(Entity<TextArea>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InsertMode {
    AtCaret,
    WholeValue,
}

impl InsertMode {
    pub(super) fn offers_formats(self) -> bool {
        self == Self::AtCaret
    }
}

impl InsertTarget {
    fn insert(&self, text: &str, mode: InsertMode, cx: &mut App) {
        match self {
            Self::Input(input) => input.update(cx, |input, cx| {
                if mode == InsertMode::WholeValue {
                    input.clear(cx);
                }
                input.insert_at_caret(text, cx);
            }),
            Self::Area(area) => area.update(cx, |area, cx| {
                if mode == InsertMode::WholeValue {
                    area.clear(cx);
                }
                area.insert_at_caret(text, cx);
            }),
        }
    }

    fn focus(&self, window: &mut Window, cx: &mut App) {
        match self {
            Self::Input(input) => input.update(cx, |input, cx| input.focus(window, cx)),
            Self::Area(area) => area.update(cx, |area, cx| area.focus(window, cx)),
        }
    }
}

struct OpenVariablePicker {
    key: String,
    target: InsertTarget,
    mode: InsertMode,
    picker: Entity<VariablePicker>,
    _sub: Subscription,
}

#[derive(Default)]
pub(super) struct VariableInsert {
    scope: Vec<ScopedVariable>,
    open: Option<OpenVariablePicker>,
}

impl EditSubActionForm {
    #[must_use]
    pub(in crate::actions_screen) fn with_variable_scope(
        mut self,
        scope: Vec<ScopedVariable>,
    ) -> Self {
        self.variables.scope = scope;
        self
    }

    pub(super) fn open_variable_picker(
        &mut self,
        key: String,
        target: InsertTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let already_open = self
            .variables
            .open
            .as_ref()
            .is_some_and(|open| open.key == key);
        if already_open {
            self.close_variable_picker(window, cx);
            return;
        }
        self.select_picker = None;
        self.datetime_picker = None;
        let mode = self.insert_mode_for(&key);
        let palette = cx.palette();
        let groups = variable_groups(
            &self.variables.scope,
            current_names_language(),
            OffsetDateTime::now_utc(),
            &palette,
        );
        let groups = if mode.offers_formats() {
            groups
        } else {
            without_formats(groups)
        };
        let labels = VariablePickerLabels {
            placeholder: tr!("actions_var_picker_search").into(),
            empty: tr!("actions_var_picker_empty").into(),
            no_format: tr!("actions_var_picker_no_format").into(),
        };
        let picker = cx.new(|cx| VariablePicker::new(labels, groups, palette, window, cx));
        let sub = cx.subscribe_in(&picker, window, Self::on_variable_picker_event);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.variables.open = Some(OpenVariablePicker {
            key,
            target,
            mode,
            picker,
            _sub: sub,
        });
        cx.notify();
    }

    pub(super) fn insert_mode_for(&self, key: &str) -> InsertMode {
        let whole_value = self.fields.iter().any(|field| {
            matches!(
                field,
                SubFormField::Input { key: k, datetime: true, .. } if k == key
            )
        });
        if whole_value {
            InsertMode::WholeValue
        } else {
            InsertMode::AtCaret
        }
    }

    fn on_variable_picker_event(
        &mut self,
        _picker: &Entity<VariablePicker>,
        event: &VariablePickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            VariablePickerEvent::Picked(token) => self.insert_variable(token, window, cx),
            VariablePickerEvent::Cancelled => self.close_variable_picker(window, cx),
        }
    }

    pub(super) fn insert_variable(
        &mut self,
        token: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = self.variables.open.take() {
            open.target.insert(token, open.mode, cx);
            open.target.focus(window, cx);
        }
        cx.notify();
    }

    fn close_variable_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.variables.open.take() {
            open.target.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn dismiss_variable_picker(&mut self, cx: &mut Context<Self>) {
        self.variables.open = None;
        cx.notify();
    }

    pub(super) fn variable_button(
        &self,
        key: &str,
        target: InsertTarget,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key_open = key.to_owned();
        ghost_button_with_icon(Icon::Variable, tr!("actions_var_picker_open"), palette)
            .ink(palette.warning)
            .on_click(
                SharedString::from(format!("actions-sub-variable-{key}")),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_variable_picker(key_open.clone(), target.clone(), window, cx)
                }),
            )
            .into_any_element()
    }

    pub(super) fn with_variable_popover(
        &self,
        key: &str,
        field: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let popover = self
            .variables
            .open
            .as_ref()
            .filter(|open| open.key == key)
            .map(|open| {
                let view = cx.entity();
                dropdown(open.picker.clone()).on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.dismiss_variable_picker(cx));
                })
            });
        div()
            .relative()
            .w_full()
            .child(field)
            .children(popover)
            .into_any_element()
    }
}

pub(super) fn datetime_presets(language: NamesLanguage) -> [&'static str; 4] {
    match language {
        NamesLanguage::Uk => UK_DATETIME_PRESETS,
        NamesLanguage::En => EN_DATETIME_PRESETS,
    }
}

pub(super) fn datetime_formats(
    name: &str,
    language: NamesLanguage,
    now: OffsetDateTime,
) -> Vec<VariableFormat> {
    let plain = VariableFormat {
        pattern: None,
        preview: datetime_to_text(&now).into(),
        token: format!("%{name}%").into(),
    };
    let presets = datetime_presets(language)
        .into_iter()
        .filter_map(|pattern| {
            let parsed = DatePattern::parse(pattern).ok()?;
            Some(VariableFormat {
                pattern: Some(pattern.into()),
                preview: parsed.format(now, PatternZone::Local, language).into(),
                token: format!("%{name}:{pattern}%").into(),
            })
        });
    std::iter::once(plain).chain(presets).collect()
}

pub(super) fn variable_groups(
    scope: &[ScopedVariable],
    language: NamesLanguage,
    now: OffsetDateTime,
    palette: &ForgePalette,
) -> Vec<VariableGroup> {
    let sections = [
        (
            tr!("actions_var_picker_group_trigger"),
            Section::AllTriggers,
        ),
        (
            tr!("actions_var_picker_group_some_triggers"),
            Section::SomeTriggers,
        ),
        (tr!("actions_var_picker_group_steps"), Section::EarlierSteps),
        (tr!("actions_var_picker_group_loop"), Section::Loop),
    ];
    sections
        .into_iter()
        .filter_map(|(title, section)| {
            let entries: Vec<VariableEntry> = scope
                .iter()
                .filter(|variable| Section::of(variable) == section)
                .map(|variable| variable_entry(variable, language, now, palette))
                .collect();
            (!entries.is_empty()).then(|| VariableGroup {
                title: title.into(),
                entries,
            })
        })
        .collect()
}

pub(super) fn without_formats(groups: Vec<VariableGroup>) -> Vec<VariableGroup> {
    groups
        .into_iter()
        .map(|group| VariableGroup {
            entries: group
                .entries
                .into_iter()
                .map(|entry| VariableEntry {
                    formats: Vec::new(),
                    ..entry
                })
                .collect(),
            ..group
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    AllTriggers,
    SomeTriggers,
    EarlierSteps,
    Loop,
}

impl Section {
    fn of(variable: &ScopedVariable) -> Self {
        match variable.origin {
            ScopeOrigin::Trigger if variable.some_triggers_only => Self::SomeTriggers,
            ScopeOrigin::Trigger => Self::AllTriggers,
            ScopeOrigin::EarlierStep => Self::EarlierSteps,
            ScopeOrigin::EnclosingBody => Self::Loop,
        }
    }
}

fn variable_entry(
    variable: &ScopedVariable,
    language: NamesLanguage,
    now: OffsetDateTime,
    palette: &ForgePalette,
) -> VariableEntry {
    let formats = if variable.kind == Some(VariantKind::Datetime) {
        datetime_formats(&variable.name, language, now)
    } else {
        Vec::new()
    };
    VariableEntry {
        name: variable.name.clone().into(),
        token: format!("%{}%", variable.name).into(),
        tag: variable
            .kind
            .map(|kind| (kind.label().into(), variant_kind_color(kind, palette))),
        formats,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_components::{
        FORGE_DEFAULT, Presentation, ThemeId, bind_picker_keys, bind_text_input_keys,
    };
    use forge_registry::FormField;
    use forge_storage::Language;
    use gpui::{TestAppContext, VisualTestContext};

    use super::super::tests::launch_with;
    use super::*;
    use crate::test_support::runtime;

    const MESSAGE: &str = "message";
    const TITLE: &str = "title";
    const BODY: &str = "body";
    const MODE: &str = "mode";
    const FIXED_INSTANT_UNIX: i64 = 1_773_568_800;

    fn fixed_instant() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(FIXED_INSTANT_UNIX).unwrap()
    }

    fn scoped(
        name: &str,
        kind: Option<VariantKind>,
        origin: ScopeOrigin,
        some_triggers_only: bool,
    ) -> ScopedVariable {
        ScopedVariable {
            name: name.to_owned(),
            kind,
            origin,
            some_triggers_only,
        }
    }

    fn tokens(formats: &[VariableFormat]) -> Vec<String> {
        formats
            .iter()
            .map(|format| format.token.to_string())
            .collect()
    }

    #[test]
    fn every_datetime_preset_is_a_valid_date_pattern() {
        for language in [NamesLanguage::En, NamesLanguage::Uk] {
            for preset in datetime_presets(language) {
                assert!(
                    DatePattern::parse(preset).is_ok(),
                    "{language:?} preset {preset:?}"
                );
            }
        }
    }

    #[test]
    fn datetime_format_tokens_lead_with_the_plain_token_then_the_language_presets() {
        for (language, expected) in [
            (
                NamesLanguage::En,
                [
                    "%stamp%",
                    "%stamp:MMMM d, HH:mm%",
                    "%stamp:HH:mm%",
                    "%stamp:MM/dd/yyyy%",
                    "%stamp:d MMMM%",
                ],
            ),
            (
                NamesLanguage::Uk,
                [
                    "%stamp%",
                    "%stamp:d MMMM, HH:mm%",
                    "%stamp:HH:mm%",
                    "%stamp:dd.MM.yyyy%",
                    "%stamp:d MMMM%",
                ],
            ),
        ] {
            let formats = datetime_formats("stamp", language, fixed_instant());
            assert_eq!(tokens(&formats), expected, "{language:?}");
        }
    }

    #[test]
    fn the_plain_datetime_row_has_no_pattern_and_previews_the_stored_text() {
        let formats = datetime_formats("stamp", NamesLanguage::En, fixed_instant());

        assert_eq!(
            (formats[0].pattern.clone(), formats[0].preview.to_string()),
            (None, "2026-03-15T10:00:00Z".to_owned())
        );
    }

    #[test]
    fn each_preset_row_previews_the_fixed_instant_through_its_own_pattern() {
        for language in [NamesLanguage::En, NamesLanguage::Uk] {
            let formats = datetime_formats("stamp", language, fixed_instant());
            for format in &formats[1..] {
                let pattern = format.pattern.clone().unwrap();
                let expected = DatePattern::parse(&pattern).unwrap().format(
                    fixed_instant(),
                    PatternZone::Local,
                    language,
                );
                assert_eq!(
                    format.preview.to_string(),
                    expected,
                    "{language:?} {pattern}"
                );
            }
        }
    }

    #[test]
    fn a_month_name_preview_follows_the_names_language() {
        let preview = |language| {
            datetime_formats("stamp", language, fixed_instant())
                .into_iter()
                .find(|format| format.pattern.as_deref() == Some("d MMMM"))
                .map(|format| format.preview.to_string())
                .unwrap()
        };

        assert_eq!(
            (
                preview(NamesLanguage::En).ends_with("March"),
                preview(NamesLanguage::Uk).ends_with("березня")
            ),
            (true, true)
        );
    }

    fn groups(scope: &[ScopedVariable]) -> Vec<VariableGroup> {
        crate::i18n::install_language(Language::En);
        variable_groups(scope, NamesLanguage::En, fixed_instant(), &FORGE_DEFAULT)
    }

    fn titles(groups: &[VariableGroup]) -> Vec<String> {
        groups.iter().map(|group| group.title.to_string()).collect()
    }

    #[test]
    fn variable_groups_follow_the_section_order_whatever_the_scope_order() {
        let scope = [
            scoped("index", None, ScopeOrigin::EnclosingBody, false),
            scoped("made", None, ScopeOrigin::EarlierStep, false),
            scoped("partial", None, ScopeOrigin::Trigger, true),
            scoped("user", None, ScopeOrigin::Trigger, false),
        ];

        assert_eq!(
            titles(&groups(&scope)),
            ["Trigger", "Only some triggers", "Earlier steps", "Loop"]
        );
    }

    #[test]
    fn a_section_without_variables_is_left_out() {
        let scope = [scoped("made", None, ScopeOrigin::EarlierStep, false)];

        assert_eq!(titles(&groups(&scope)), ["Earlier steps"]);
    }

    #[test]
    fn an_entry_tag_carries_the_kind_label_in_the_kind_color() {
        for (kind, expected) in [
            (
                Some(VariantKind::Int),
                Some((
                    SharedString::from("INT"),
                    variant_kind_color(VariantKind::Int, &FORGE_DEFAULT),
                )),
            ),
            (None, None),
        ] {
            let scope = [scoped("value", kind, ScopeOrigin::EarlierStep, false)];
            let entry = groups(&scope)[0].entries[0].clone();
            assert_eq!(entry.tag, expected, "{kind:?}");
        }
    }

    #[test]
    fn only_a_datetime_entry_opens_a_format_submenu() {
        for (kind, has_formats) in [
            (Some(VariantKind::Datetime), true),
            (Some(VariantKind::String), false),
            (None, false),
        ] {
            let scope = [scoped("value", kind, ScopeOrigin::EarlierStep, false)];
            let entry = groups(&scope)[0].entries[0].clone();
            assert_eq!(!entry.formats.is_empty(), has_formats, "{kind:?}");
        }
    }

    fn text(key: &'static str) -> FormField {
        FormField::Text {
            key,
            label: key,
            placeholder: "",
        }
    }

    fn text_specs() -> Vec<FormField> {
        vec![
            text(MESSAGE),
            text(TITLE),
            FormField::Select {
                key: MODE,
                label: MODE,
                options: &["on", "off"],
            },
        ]
    }

    fn stored(pairs: &[(&str, &str)]) -> SubActionConfig {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
            .collect()
    }

    fn open_form(
        cx: &mut TestAppContext,
        specs: Vec<FormField>,
        config: SubActionConfig,
    ) -> (
        Entity<EditSubActionForm>,
        &mut VisualTestContext,
        tokio::runtime::Runtime,
    ) {
        crate::i18n::install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            bind_picker_keys(cx);
            bind_text_input_keys(cx);
        });
        let rt = runtime();
        let handle = rt.handle().clone();
        let launch = launch_with(specs, config);
        let scope = vec![scoped(
            "user",
            Some(VariantKind::String),
            ScopeOrigin::Trigger,
            false,
        )];
        let (form, vcx) = cx.add_window_view(move |_window, cx| {
            EditSubActionForm::new(launch, handle, cx).with_variable_scope(scope)
        });
        vcx.run_until_parked();
        (form, vcx, rt)
    }

    fn target_of(
        form: &Entity<EditSubActionForm>,
        vcx: &mut VisualTestContext,
        key: &str,
    ) -> InsertTarget {
        vcx.update(|_window, cx| {
            form.read(cx)
                .fields
                .iter()
                .find_map(|field| match field {
                    SubFormField::Input { key: k, input, .. } if k == key => {
                        Some(InsertTarget::Input(input.clone()))
                    }
                    SubFormField::Area { key: k, area, .. } if k == key => {
                        Some(InsertTarget::Area(area.clone()))
                    }
                    _ => None,
                })
                .unwrap()
        })
    }

    fn content_of(
        form: &Entity<EditSubActionForm>,
        vcx: &mut VisualTestContext,
        key: &str,
    ) -> String {
        let target = target_of(form, vcx, key);
        vcx.update(|_window, cx| match target {
            InsertTarget::Input(input) => input.read(cx).content().to_owned(),
            InsertTarget::Area(area) => area.read(cx).content().to_owned(),
        })
    }

    fn open_picker(form: &Entity<EditSubActionForm>, vcx: &mut VisualTestContext, key: &str) {
        let target = target_of(form, vcx, key);
        vcx.update(|window, cx| {
            form.update(cx, |form, cx| {
                form.open_variable_picker(key.to_owned(), target, window, cx)
            })
        });
        vcx.run_until_parked();
    }

    fn open_key(form: &Entity<EditSubActionForm>, vcx: &mut VisualTestContext) -> Option<String> {
        vcx.update(|_window, cx| {
            form.read(cx)
                .variables
                .open
                .as_ref()
                .map(|open| open.key.clone())
        })
    }

    #[gpui::test]
    fn only_free_text_fields_accept_variables(cx: &mut TestAppContext) {
        let specs = vec![
            text(MESSAGE),
            text("into_var"),
            FormField::Integer {
                key: "count",
                label: "count",
                min: 0,
                max: 9,
            },
            FormField::FilePicker {
                key: "file",
                label: "file",
            },
            FormField::DateTime {
                key: "when",
                label: "when",
            },
            FormField::TextArea {
                key: BODY,
                label: BODY,
            },
            FormField::Toggle {
                key: "flag",
                label: "flag",
            },
        ];
        let (form, vcx, _rt) = open_form(cx, specs, SubActionConfig::new());

        let accepting: Vec<String> = vcx.update(|_window, cx| {
            form.read(cx)
                .fields
                .iter()
                .filter(|field| field.accepts_variables())
                .filter_map(|field| match field {
                    SubFormField::Input { key, .. } | SubFormField::Area { key, .. } => {
                        Some(key.clone())
                    }
                    _ => None,
                })
                .collect()
        });

        assert_eq!(accepting, [MESSAGE, BODY]);
    }

    #[gpui::test]
    fn reopening_the_picker_on_the_same_field_closes_it(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open_form(cx, text_specs(), SubActionConfig::new());

        open_picker(&form, vcx, MESSAGE);
        open_picker(&form, vcx, MESSAGE);

        assert_eq!(open_key(&form, vcx), None);
    }

    #[gpui::test]
    fn opening_the_picker_on_another_field_moves_it_there(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open_form(cx, text_specs(), SubActionConfig::new());

        open_picker(&form, vcx, MESSAGE);
        open_picker(&form, vcx, TITLE);

        assert_eq!(open_key(&form, vcx), Some(TITLE.to_owned()));
    }

    #[gpui::test]
    fn opening_the_variable_picker_closes_an_open_select_picker(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open_form(cx, text_specs(), SubActionConfig::new());
        vcx.update(|window, cx| {
            form.update(cx, |form, cx| {
                form.open_select_picker(MODE.to_owned(), window, cx)
            })
        });

        open_picker(&form, vcx, MESSAGE);

        let select_open = vcx.update(|_window, cx| form.read(cx).select_picker.is_some());
        assert!(!select_open);
    }

    #[gpui::test]
    fn picking_a_variable_inserts_its_token_into_the_field_it_was_opened_for(
        cx: &mut TestAppContext,
    ) {
        let (form, vcx, _rt) =
            open_form(cx, text_specs(), stored(&[(MESSAGE, "hi "), (TITLE, "T ")]));
        open_picker(&form, vcx, TITLE);

        vcx.simulate_keystrokes("enter");
        vcx.run_until_parked();

        assert_eq!(
            (
                content_of(&form, vcx, TITLE),
                content_of(&form, vcx, MESSAGE),
                open_key(&form, vcx)
            ),
            ("T %user%".to_owned(), "hi ".to_owned(), None)
        );
    }

    #[gpui::test]
    fn picking_a_variable_for_a_text_area_inserts_into_the_area(cx: &mut TestAppContext) {
        let specs = vec![FormField::TextArea {
            key: BODY,
            label: BODY,
        }];
        let (form, vcx, _rt) = open_form(cx, specs, stored(&[(BODY, "line\n")]));
        open_picker(&form, vcx, BODY);

        vcx.simulate_keystrokes("enter");
        vcx.run_until_parked();

        assert_eq!(content_of(&form, vcx, BODY), "line\n%user%");
    }

    #[gpui::test]
    fn submitting_after_a_pick_commits_the_inserted_token(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open_form(cx, text_specs(), stored(&[(MESSAGE, "hi ")]));
        open_picker(&form, vcx, MESSAGE);
        vcx.simulate_keystrokes("enter");
        vcx.run_until_parked();
        let heard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = vcx.update(|_window, cx| {
            cx.subscribe(&form, move |_, event: &SubFormEvent, _| {
                if let SubFormEvent::Commit(commit) = event {
                    *sink.borrow_mut() = Some(commit.overrides.clone());
                }
            })
        });

        vcx.update(|_window, cx| form.update(cx, |form, cx| form.submit(cx)));

        let committed = heard.borrow().as_ref().and_then(|overrides| {
            overrides
                .iter()
                .find(|(key, _)| key == MESSAGE)
                .map(|(_, value)| value.clone())
        });
        assert_eq!(committed, Some(Variant::String("hi %user%".to_owned())));
    }
}
