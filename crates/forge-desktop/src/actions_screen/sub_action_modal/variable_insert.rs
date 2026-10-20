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

impl InsertTarget {
    fn insert(&self, text: &str, cx: &mut App) {
        match self {
            Self::Input(input) => input.update(cx, |input, cx| input.insert_at_caret(text, cx)),
            Self::Area(area) => area.update(cx, |area, cx| area.insert_at_caret(text, cx)),
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
        let palette = cx.palette();
        let groups = variable_groups(
            &self.variables.scope,
            current_names_language(),
            OffsetDateTime::now_utc(),
            &palette,
        );
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
            picker,
            _sub: sub,
        });
        cx.notify();
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
            open.target.insert(token, cx);
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

    fn dismiss_variable_picker(&mut self, cx: &mut Context<Self>) {
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
