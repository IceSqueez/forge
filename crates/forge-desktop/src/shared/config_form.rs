use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::ops::RangeInclusive;
use std::rc::Rc;

use forge_components::{
    CodeEditor, ForgePalette, InputEvent, Picker, Radius, TextArea, TextInput, radius,
};
use forge_registry::{CodeKind, CodeLanguage};

use crate::automation::amount_scale::AmountScale;

use crate::shared::option_choices::ChoiceOption;
use forge_types::Variant;
use gpui::{App, Context, Entity, Pixels, Subscription, Window, px};

mod choices;
mod collect;
mod fold;
mod render;

pub(crate) use choices::{
    CollectionChoices, choice_entries, dependent_options, dependent_options_key,
    resolve_dependent_choices,
};
pub(crate) use collect::{
    CollectedValues, GateStates, collect_field_values, field_entries, gated_field_values,
};
use fold::settle_integer_input;
pub(crate) use fold::{flag_integer_input, flag_variable_input, fold_config_field};
pub(crate) use render::{config_label_cell, render_config_control, render_config_row};

const FILL_KEY_W: Pixels = px(110.0);
const FILL_KEY_FS: Pixels = FONT_META;
pub(crate) const FILL_VAL_FS: Pixels = FONT_LABEL;
const FILL_ROW_PAD_V: Pixels = px(8.0);
const FILL_ROW_PAD_H: Pixels = px(12.0);

const SLIDER_READOUT_W: Pixels = px(34.0);

const SWATCH_SIZE: Pixels = px(22.0);
const SWATCH_RADIUS: Pixels = radius(Radius::Sm);
const SWATCH_RING: Pixels = px(2.0);

pub(crate) const CHOICE_PAD_V: Pixels = px(6.0);
pub(crate) const CHOICE_PAD_H: Pixels = px(9.0);
pub(crate) const CHOICE_GLYPH: Pixels = px(12.0);

pub(crate) const INTEGER_PLACEHOLDER: &str = "0";
const VAR_PLACEHOLDER: &str = "%result%";

const CODE_FIELD_ROWS: RangeInclusive<usize> = 2..=12;
const AREA_FIELD_H: Pixels = px(150.0);

type FieldConfig = BTreeMap<String, Variant>;

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum InputFlavor {
    Text,
    Integer(RangeInclusive<i64>),
    OneBased(i64),
    File,
    DateTime,
    VariableName,
}

impl InputFlavor {
    pub(crate) fn shown_bounds(&self) -> Option<RangeInclusive<i64>> {
        match self {
            Self::Integer(range) => Some(range.clone()),
            Self::OneBased(count) => Some(1..=*count),
            Self::Text | Self::File | Self::DateTime | Self::VariableName => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryPolicy {
    Inline,
    OnSubmit,
}

#[derive(Clone, Copy)]
pub(crate) struct FieldCapabilities<'a> {
    pub(crate) choices: ChoiceSupport<'a>,
    pub(crate) entry: EntryPolicy,
    pub(crate) text_areas: bool,
    pub(crate) sliders: bool,
    pub(crate) swatches: bool,
    pub(crate) output_name_keys: &'a [String],
}

impl<'a> FieldCapabilities<'a> {
    pub(crate) fn inline(choices: ChoiceSupport<'a>) -> Self {
        Self {
            choices,
            entry: EntryPolicy::Inline,
            text_areas: false,
            sliders: true,
            swatches: true,
            output_name_keys: &[],
        }
    }

    pub(crate) fn on_submit(
        options: &'a HashMap<String, Vec<ChoiceOption>>,
        output_name_keys: &'a [String],
    ) -> Self {
        Self {
            choices: ChoiceSupport::Picker(options),
            entry: EntryPolicy::OnSubmit,
            text_areas: true,
            sliders: false,
            swatches: false,
            output_name_keys,
        }
    }
}

pub(crate) enum ConfigField {
    Input {
        key: String,
        flavor: InputFlavor,
        optional: bool,
        gate: Option<String>,
        input: Entity<TextInput>,
        _sub: Option<Subscription>,
    },
    Area {
        key: String,
        gate: Option<String>,
        area: Entity<TextArea>,
    },
    Code {
        key: String,
        gate: Option<String>,
        language: CodeLanguage,
        kind: CodeKind,
        editor: Entity<CodeEditor>,
        _sub: Option<Subscription>,
    },
    Bool {
        key: String,
        gate: Option<String>,
        value: bool,
        presence_gate: bool,
    },
    Slide {
        key: String,
        gate: Option<String>,
        min: i64,
        max: i64,
        unit: &'static str,
        value: i64,
    },
    Swatch {
        key: String,
        gate: Option<String>,
        options: Vec<(String, gpui::Rgba)>,
        selected: String,
    },
    Choice {
        key: String,
        gate: Option<String>,
        options: Vec<ChoiceOption>,
        options_key: Option<String>,
        selected: String,
        dependency: Option<ChoiceDependency>,
    },
    UnitAmount {
        key: String,
        unit_picker_key: String,
        gate: Option<String>,
        scale: AmountScale,
        units: Vec<ChoiceOption>,
        unit: String,
        live_range: Rc<Cell<(i64, i64)>>,
        policy: EntryPolicy,
        input: Entity<TextInput>,
        _sub: Subscription,
    },
    Hint {
        key: String,
    },
}

pub(crate) struct ChoiceDependency {
    pub(crate) options_prefix: String,
    pub(crate) depends_on: String,
}

impl ConfigField {
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Input { key, .. }
            | Self::Area { key, .. }
            | Self::Code { key, .. }
            | Self::Bool { key, .. }
            | Self::Slide { key, .. }
            | Self::Swatch { key, .. }
            | Self::Choice { key, .. }
            | Self::UnitAmount { key, .. }
            | Self::Hint { key } => key,
        }
    }

    pub(crate) fn gate(&self) -> Option<&String> {
        match self {
            Self::Input { gate, .. }
            | Self::Area { gate, .. }
            | Self::Code { gate, .. }
            | Self::Bool { gate, .. }
            | Self::Slide { gate, .. }
            | Self::Swatch { gate, .. }
            | Self::Choice { gate, .. }
            | Self::UnitAmount { gate, .. } => gate.as_ref(),
            Self::Hint { .. } => None,
        }
    }

    pub(crate) fn stored_keys(&self) -> Vec<&str> {
        match self {
            Self::UnitAmount { key, scale, .. } => scale.stored_keys(key),
            Self::Hint { .. } => Vec::new(),
            other => vec![other.key()],
        }
    }

    pub(crate) fn accepts_variables(&self) -> bool {
        match self {
            Self::Input { flavor, .. } => *flavor == InputFlavor::Text,
            Self::Area { .. }
            | Self::Code {
                kind: CodeKind::Expression,
                ..
            } => true,
            _ => false,
        }
    }

    pub(crate) fn flag_unit_amount_bound(&self, cx: &mut App) -> bool {
        let Self::UnitAmount {
            live_range, input, ..
        } = self
        else {
            return false;
        };
        let (min, max) = live_range.get();
        flag_integer_input(input, &(min..=max), cx)
    }
}

pub(crate) type ConfigCommitHandler<V> = fn(&mut V, &InputEvent, &mut Context<V>);

type ChoiceOpener<V> = fn(&mut V, String, &mut Window, &mut Context<V>);

type ChoiceCloser<V> = fn(&mut V, &mut Context<V>);

pub(crate) struct ChoiceDropdown<V: 'static> {
    pub(crate) open: ChoiceOpener<V>,
    pub(crate) close: ChoiceCloser<V>,
    pub(crate) active: Option<(String, Entity<Picker>)>,
}

#[derive(Clone, Copy)]
pub(crate) enum ChoiceSupport<'a> {
    Text,
    Picker(&'a HashMap<String, Vec<ChoiceOption>>),
}

pub(crate) struct ConfigFieldHandlers<V: 'static> {
    pub(crate) toggle: fn(&mut V, String, &mut Context<V>),
    pub(crate) slide: fn(&mut V, String, i64, &mut Context<V>),
    pub(crate) pick: fn(&mut V, String, String, &mut Context<V>),
    pub(crate) choice: Option<ChoiceDropdown<V>>,
}

pub(crate) fn sparse_overrides(default: &FieldConfig, buffer: &FieldConfig) -> FieldConfig {
    buffer
        .iter()
        .filter(|(k, v)| default.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub(crate) struct FoldContext<'a, V: 'static> {
    pub(crate) kind_id: &'a str,
    pub(crate) config: &'a FieldConfig,
    pub(crate) defaults: &'a FieldConfig,
    pub(crate) palette: &'a ForgePalette,
    pub(crate) capabilities: FieldCapabilities<'a>,
    pub(crate) on_committed: ConfigCommitHandler<V>,
}

pub(crate) fn toggle_bool_field(fields: &mut [ConfigField], key: &str) {
    for field in fields {
        if let ConfigField::Bool { key: k, value, .. } = field
            && k == key
        {
            *value = !*value;
        }
    }
}

pub(crate) fn set_slide_field(fields: &mut [ConfigField], key: &str, next: i64) {
    for field in fields {
        if let ConfigField::Slide { key: k, value, .. } = field
            && k == key
        {
            *value = next;
        }
    }
}

pub(crate) fn set_picked_value(fields: &mut [ConfigField], key: &str, value: &str, cx: &mut App) {
    for field in fields {
        match field {
            ConfigField::Swatch {
                key: k, selected, ..
            }
            | ConfigField::Choice {
                key: k, selected, ..
            } if k == key => value.clone_into(selected),
            ConfigField::UnitAmount {
                unit_picker_key,
                scale,
                unit,
                live_range,
                policy,
                input,
                ..
            } if unit_picker_key == key => {
                let typed = input.read(cx).content().to_owned();
                if let Some(converted) = scale.amount_after_unit_switch(&typed, unit, value) {
                    input.update(cx, |input, cx| input.set_content(converted, cx));
                }
                value.clone_into(unit);
                let range = scale.range_for(unit);
                live_range.set((*range.start(), *range.end()));
                if *policy == EntryPolicy::Inline {
                    let content = input.read(cx).content().to_owned();
                    settle_integer_input(input, &content, &range, cx);
                } else {
                    flag_integer_input(input, &range, cx);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fold::clamped_integer_text;
    use super::*;
    use crate::shared::option_choices::ChoiceOptions;
    use forge_registry::{FormField, UnitAmountBounds};
    use forge_runtime::DONATION_PROVIDER_OPTIONS_KEY;
    use gpui::prelude::*;

    mod live_pickers;

    fn config(entries: &[(&str, Variant)]) -> FieldConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    #[test]
    fn a_value_equal_to_the_kind_default_is_not_stored_as_an_override() {
        let default = config(&[
            ("accent", Variant::String("mauve".into())),
            ("duration", Variant::Int(5)),
            ("sticky", Variant::Bool(false)),
        ]);
        let buffer = config(&[
            ("accent", Variant::String("mauve".into())),
            ("duration", Variant::Int(9)),
            ("sticky", Variant::Bool(false)),
            ("headline", Variant::String("%user%".into())),
        ]);

        assert_eq!(
            sparse_overrides(&default, &buffer),
            config(&[
                ("duration", Variant::Int(9)),
                ("headline", Variant::String("%user%".into())),
            ]),
            "a record must carry only what the user moved away from the kind default"
        );
    }

    #[test]
    fn a_value_that_only_looks_like_the_default_is_still_an_override() {
        let default = config(&[("duration", Variant::Int(5))]);
        let buffer = config(&[("duration", Variant::String("5".into()))]);

        assert_eq!(
            sparse_overrides(&default, &buffer),
            buffer,
            "a value stored under a different type is not the default, however it displays"
        );
    }

    #[test]
    fn a_default_the_buffer_never_mentions_is_not_resurrected_as_an_override() {
        let default = config(&[
            ("accent", Variant::String("mauve".into())),
            ("duration", Variant::Int(5)),
        ]);

        assert!(
            sparse_overrides(&default, &FieldConfig::new()).is_empty(),
            "an empty form means no overrides, not every default written out longhand"
        );
    }

    fn options_map(entries: &[(&str, &[(&str, &str)])]) -> HashMap<String, Vec<ChoiceOption>> {
        entries
            .iter()
            .map(|(key, options)| {
                (
                    (*key).to_owned(),
                    options
                        .iter()
                        .map(|(value, label)| ChoiceOption::new(*value, *label))
                        .collect(),
                )
            })
            .collect()
    }

    fn voice_options() -> HashMap<String, Vec<ChoiceOption>> {
        options_map(&[
            ("tts.voices", &[("bare", "Bare prefix")]),
            ("tts.voices.", &[("empty-engine", "Empty engine")]),
            ("tts.voicespiper", &[("glued", "Glued key")]),
            ("tts.voices.piper", &[("amy", "Amy"), ("alan", "Alan")]),
            ("tts.voices.azure", &[("aria", "Aria")]),
        ])
    }

    #[test]
    fn a_dependent_list_is_read_from_the_prefix_joined_to_the_sibling_by_a_dot() {
        assert_eq!(
            dependent_options(&voice_options(), "tts.voices", "piper"),
            vec![
                ChoiceOption::new("amy", "Amy"),
                ChoiceOption::new("alan", "Alan"),
            ],
            "the chosen engine selects its own voices, not the bare prefix and not a glued key"
        );
    }

    #[test]
    fn a_dependent_list_stays_empty_while_the_sibling_names_nothing_known() {
        for sibling in ["", "espeak"] {
            assert!(
                dependent_options(&voice_options(), "tts.voices", sibling).is_empty(),
                "engine {sibling:?} has no registered voices, so the dropdown must offer none"
            );
        }
    }

    #[gpui::test]
    fn clearing_a_choice_erases_the_stored_value_while_clearing_a_swatch_keeps_it(
        cx: &mut gpui::TestAppContext,
    ) {
        let seeded = config(&[
            ("sound", Variant::String("clip:01J9P4S2M7".into())),
            ("accent", Variant::String("mauve".into())),
        ]);
        let cleared = vec![
            choice("sound", "", None),
            ConfigField::Swatch {
                key: "accent".to_owned(),
                gate: None,
                options: Vec::new(),
                selected: String::new(),
            },
        ];

        assert_eq!(
            collected(cx, &cleared, &seeded),
            config(&[
                ("sound", Variant::String(String::new())),
                ("accent", Variant::String("mauve".into())),
            ]),
            "picking the empty option must unset the value, while a swatch with nothing selected keeps it"
        );
    }

    fn choice(key: &str, selected: &str, depends_on: Option<&str>) -> ConfigField {
        ConfigField::Choice {
            key: key.to_owned(),
            gate: None,
            options: Vec::new(),
            options_key: None,
            selected: selected.to_owned(),
            dependency: depends_on.map(|depends_on| ChoiceDependency {
                options_prefix: "tts.voices".to_owned(),
                depends_on: depends_on.to_owned(),
            }),
        }
    }

    fn offered_values(field: &ConfigField) -> Vec<String> {
        match field {
            ConfigField::Choice { options, .. } => {
                options.iter().map(|option| option.value.clone()).collect()
            }
            _ => panic!("expected a choice field"),
        }
    }

    fn selected_value(field: &ConfigField) -> String {
        match field {
            ConfigField::Choice { selected, .. } => selected.clone(),
            _ => panic!("expected a choice field"),
        }
    }

    fn set_selected(field: &mut ConfigField, value: &str) {
        match field {
            ConfigField::Choice { selected, .. } => value.clone_into(selected),
            _ => panic!("expected a choice field"),
        }
    }

    #[gpui::test]
    fn a_dependent_choice_reoffers_its_list_when_the_sibling_moves(cx: &mut gpui::TestAppContext) {
        let map = voice_options();
        let mut fields = vec![
            choice("engine_id", "piper", None),
            choice("voice_id", "", Some("engine_id")),
        ];

        cx.update(|cx| {
            resolve_dependent_choices(&mut fields, &map, cx);
            assert_eq!(
                offered_values(&fields[1]),
                ["amy", "alan"],
                "the voice list must follow the engine the form currently holds"
            );

            set_selected(&mut fields[0], "azure");
            resolve_dependent_choices(&mut fields, &map, cx);
            assert_eq!(
                offered_values(&fields[1]),
                ["aria"],
                "switching engines must re-offer the new engine's voices"
            );
        });
    }

    #[gpui::test]
    fn a_selection_the_new_sibling_no_longer_offers_is_left_standing(
        cx: &mut gpui::TestAppContext,
    ) {
        let map = voice_options();
        let mut fields = vec![
            choice("engine_id", "azure", None),
            choice("voice_id", "amy", Some("engine_id")),
        ];

        cx.update(|cx| {
            resolve_dependent_choices(&mut fields, &map, cx);

            assert_eq!(
                selected_value(&fields[1]),
                "amy",
                "reoffering options must not silently rewrite or clear what the user picked"
            );
        });
    }

    const NOTE_KEY: &str = "note";
    const SEEDED: i64 = 9;

    struct Host;

    fn ignore_commit(_: &mut Host, _: &InputEvent, _: &mut Context<Host>) {}

    fn integer_spec(key: &'static str) -> FormField {
        FormField::Integer {
            key,
            label: "Note",
            min: 0,
            max: 127,
        }
    }

    fn optional_integer_spec(key: &'static str) -> FormField {
        FormField::Optional {
            key,
            label: "Note",
            inner: Box::new(integer_spec(key)),
        }
    }

    fn build(
        cx: &mut gpui::TestAppContext,
        spec: &FormField,
        defaults: &FieldConfig,
        config: &FieldConfig,
        typed: &str,
    ) -> (gpui::Entity<Host>, Vec<ConfigField>) {
        let (host, fields) = fold_untouched(cx, spec, defaults, config);
        host.update(cx, |_, cx| {
            for field in &fields {
                if let ConfigField::Input { input, .. } = field {
                    input.update(cx, |field, cx| field.set_content(typed.to_owned(), cx));
                }
            }
        });
        (host, fields)
    }

    fn fold_untouched(
        cx: &mut gpui::TestAppContext,
        spec: &FormField,
        defaults: &FieldConfig,
        config: &FieldConfig,
    ) -> (gpui::Entity<Host>, Vec<ConfigField>) {
        let host = cx.update(|cx| cx.new(|_| Host));
        let fields = host.update(cx, |_, cx| {
            let palette = forge_components::ThemeId::ForgeDefault.palette();
            let ctx = FoldContext {
                kind_id: "",
                config,
                defaults,
                palette: &palette,
                capabilities: FieldCapabilities::inline(ChoiceSupport::Text),
                on_committed: ignore_commit as ConfigCommitHandler<Host>,
            };
            let mut fields = Vec::new();
            fold_config_field(spec, None, &ctx, &mut fields, cx);
            fields
        });
        (host, fields)
    }

    fn collected(
        cx: &mut gpui::TestAppContext,
        fields: &[ConfigField],
        seed: &FieldConfig,
    ) -> FieldConfig {
        let mut buffer = seed.clone();
        cx.update(|cx| collect_field_values(fields, &mut buffer, cx));
        buffer
    }

    #[gpui::test]
    fn an_integer_field_writes_only_a_parsed_number_and_clears_only_an_optional_key(
        cx: &mut gpui::TestAppContext,
    ) {
        for (declared_default, typed, expected) in [
            (false, "48", Some(Variant::Int(48))),
            (false, "", None),
            (false, "   ", None),
            (false, "abc", Some(Variant::Int(SEEDED))),
            (false, "4.5", Some(Variant::Int(SEEDED))),
            (true, "48", Some(Variant::Int(48))),
            (true, "", Some(Variant::Int(SEEDED))),
            (true, "abc", Some(Variant::Int(SEEDED))),
        ] {
            let defaults = if declared_default {
                config(&[(NOTE_KEY, Variant::Int(0))])
            } else {
                FieldConfig::new()
            };
            let seeded = config(&[(NOTE_KEY, Variant::Int(SEEDED))]);
            let (_host, fields) = build(cx, &integer_spec(NOTE_KEY), &defaults, &seeded, typed);

            assert_eq!(
                collected(cx, &fields, &seeded).get(NOTE_KEY),
                expected.as_ref(),
                "declared default: {declared_default}, typed: {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn an_integer_field_saves_a_number_clamped_into_its_bounds(cx: &mut gpui::TestAppContext) {
        for (typed, expected) in [
            ("128", 127),
            ("1000000", 127),
            ("127", 127),
            ("126", 126),
            ("0", 0),
            ("1", 1),
            ("-1", 0),
            (" 200 ", 127),
        ] {
            let seeded = config(&[(NOTE_KEY, Variant::Int(SEEDED))]);
            let (_host, fields) = build(
                cx,
                &integer_spec(NOTE_KEY),
                &FieldConfig::new(),
                &seeded,
                typed,
            );

            assert_eq!(
                collected(cx, &fields, &seeded).get(NOTE_KEY),
                Some(&Variant::Int(expected)),
                "typed: {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn an_integer_spec_with_min_above_max_saves_a_number_without_panicking(
        cx: &mut gpui::TestAppContext,
    ) {
        let inverted = FormField::Integer {
            key: NOTE_KEY,
            label: "Note",
            min: 10,
            max: 5,
        };
        let (_host, fields) = build(cx, &inverted, &FieldConfig::new(), &FieldConfig::new(), "7");

        assert!(matches!(
            collected(cx, &fields, &FieldConfig::new()).get(NOTE_KEY),
            Some(Variant::Int(_))
        ));
    }

    #[test]
    fn clamped_integer_text_rewrites_only_an_out_of_range_number() {
        let bounds = 0..=127;
        for (typed, expected) in [
            ("128", Some("127")),
            ("-1", Some("0")),
            (" 300 ", Some("127")),
            ("0", None),
            ("127", None),
            ("64", None),
            ("", None),
            ("abc", None),
            ("4.5", None),
        ] {
            assert_eq!(
                clamped_integer_text(typed, &bounds).as_deref(),
                expected,
                "typed: {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn leaving_or_submitting_an_out_of_range_number_snaps_the_text_to_the_bound(
        cx: &mut gpui::TestAppContext,
    ) {
        for (typed, shown) in [("500", "127"), ("-3", "0"), ("42", "42"), ("abc", "abc")] {
            for leave in [InputEvent::Submitted, InputEvent::Blurred] {
                let (_host, fields) = build(
                    cx,
                    &integer_spec(NOTE_KEY),
                    &FieldConfig::new(),
                    &FieldConfig::new(),
                    typed,
                );
                let ConfigField::Input { input, .. } = &fields[0] else {
                    panic!("an integer spec builds an input field");
                };
                input.update(cx, |_, cx| cx.emit(leave(typed.into())));
                cx.run_until_parked();

                assert_eq!(
                    input.read_with(cx, |field, _| field.content().to_owned()),
                    shown
                );
            }
        }
    }

    #[gpui::test]
    fn emptying_an_optional_number_leaves_every_other_key_standing(cx: &mut gpui::TestAppContext) {
        let seeded = config(&[
            (NOTE_KEY, Variant::Int(SEEDED)),
            ("device", Variant::String("Launchkey".into())),
        ]);
        let (_host, fields) = build(
            cx,
            &integer_spec(NOTE_KEY),
            &FieldConfig::new(),
            &seeded,
            "",
        );

        assert_eq!(
            collected(cx, &fields, &seeded),
            config(&[("device", Variant::String("Launchkey".into()))])
        );
    }

    const DEVICE_KEY: &str = "device";
    const NOTE_GATE: &str = "use_note";

    fn optional_text_spec(key: &'static str) -> FormField {
        FormField::Optional {
            key,
            label: "Device",
            inner: Box::new(FormField::Text {
                key,
                label: "Device",
                placeholder: "",
            }),
        }
    }

    fn same_key_amount_spec() -> FormField {
        FormField::Optional {
            key: AMOUNT_KEY,
            label: "Amount",
            inner: Box::new(unit_amount_spec()),
        }
    }

    fn toggle_of(fields: &[ConfigField], key: &str) -> bool {
        fields
            .iter()
            .find_map(|field| match field {
                ConfigField::Bool { key: k, value, .. } if k == key => Some(*value),
                _ => None,
            })
            .unwrap_or_else(|| panic!("an optional builds its toggle"))
    }

    fn flip_toggle(fields: &mut [ConfigField], key: &str) {
        for field in fields {
            if let ConfigField::Bool { key: k, value, .. } = field
                && k == key
            {
                *value = !*value;
            }
        }
    }

    #[gpui::test]
    fn a_same_key_optional_keeps_its_stored_value_through_an_untouched_save(
        cx: &mut gpui::TestAppContext,
    ) {
        for (spec, stored) in [
            (
                optional_integer_spec(NOTE_KEY),
                config(&[(NOTE_KEY, Variant::Int(48))]),
            ),
            (
                optional_integer_spec(NOTE_KEY),
                config(&[(NOTE_KEY, Variant::Int(0))]),
            ),
            (
                optional_text_spec(DEVICE_KEY),
                config(&[(DEVICE_KEY, Variant::String("Launchkey".into()))]),
            ),
            (
                same_key_amount_spec(),
                config(&[
                    (AMOUNT_KEY, Variant::Int(2)),
                    (AMOUNT_UNIT_KEY, Variant::String("minutes".into())),
                ]),
            ),
        ] {
            let (_host, fields) = fold_untouched(cx, &spec, &FieldConfig::new(), &stored);

            assert_eq!(
                (
                    toggle_of(&fields, spec.key()),
                    collected(cx, &fields, &stored)
                ),
                (true, stored.clone()),
                "stored {stored:?}"
            );
        }
    }

    #[gpui::test]
    fn switching_a_same_key_optional_off_removes_its_keys_and_writes_no_toggle(
        cx: &mut gpui::TestAppContext,
    ) {
        let unrelated = (NOTE_GATE, Variant::String("kept".into()));
        for (spec, gated) in [
            (
                optional_integer_spec(NOTE_KEY),
                vec![(NOTE_KEY, Variant::Int(48))],
            ),
            (
                optional_text_spec(DEVICE_KEY),
                vec![(DEVICE_KEY, Variant::String("Launchkey".into()))],
            ),
            (
                same_key_amount_spec(),
                vec![
                    (AMOUNT_KEY, Variant::Int(2)),
                    (AMOUNT_UNIT_KEY, Variant::String("minutes".into())),
                ],
            ),
        ] {
            let mut entries = gated;
            entries.push(unrelated.clone());
            let stored = config(&entries);
            let (_host, mut fields) = fold_untouched(cx, &spec, &FieldConfig::new(), &stored);
            flip_toggle(&mut fields, spec.key());

            assert_eq!(
                collected(cx, &fields, &stored),
                config(std::slice::from_ref(&unrelated)),
                "gate {}",
                spec.key()
            );
        }
    }

    #[gpui::test]
    fn a_same_key_optional_holding_a_toggle_opens_off_and_saves_without_the_key(
        cx: &mut gpui::TestAppContext,
    ) {
        for stored in [Variant::Bool(true), Variant::Bool(false)] {
            let seeded = config(&[(NOTE_KEY, stored.clone())]);
            let (_host, fields) = fold_untouched(
                cx,
                &optional_integer_spec(NOTE_KEY),
                &FieldConfig::new(),
                &seeded,
            );

            assert_eq!(
                (
                    toggle_of(&fields, NOTE_KEY),
                    collected(cx, &fields, &seeded)
                ),
                (false, FieldConfig::new()),
                "stored {stored:?}"
            );
        }
    }

    #[gpui::test]
    fn a_distinct_key_optional_writes_its_own_toggle_and_keeps_the_inner_value_when_off(
        cx: &mut gpui::TestAppContext,
    ) {
        let spec = FormField::Optional {
            key: NOTE_GATE,
            label: "Use note",
            inner: Box::new(integer_spec(NOTE_KEY)),
        };
        for (stored_toggle, flipped, saved_toggle) in [
            (true, false, true),
            (false, false, false),
            (true, true, false),
            (false, true, true),
        ] {
            let seeded = config(&[
                (NOTE_GATE, Variant::Bool(stored_toggle)),
                (NOTE_KEY, Variant::Int(48)),
            ]);
            let (_host, mut fields) = fold_untouched(cx, &spec, &FieldConfig::new(), &seeded);
            if flipped {
                flip_toggle(&mut fields, NOTE_GATE);
            }

            assert_eq!(
                collected(cx, &fields, &seeded),
                config(&[
                    (NOTE_GATE, Variant::Bool(saved_toggle)),
                    (NOTE_KEY, Variant::Int(48)),
                ]),
                "stored {stored_toggle}, flipped {flipped}"
            );
        }
    }

    fn dynamic_select(options_key: &'static str) -> FormField {
        FormField::DynamicSelect {
            key: "target",
            label: "Target",
            options_key,
        }
    }

    #[gpui::test]
    fn a_stored_raw_reward_id_survives_a_reload_and_resave_unchanged(
        cx: &mut gpui::TestAppContext,
    ) {
        let stored = config(&[("target", Variant::String("%reward.id%".into()))]);
        let (_host, fields) = build(
            cx,
            &dynamic_select("collections.twitch.rewards.manageable"),
            &FieldConfig::new(),
            &stored,
            "",
        );

        assert_eq!(collected(cx, &fields, &FieldConfig::new()), stored);
    }

    #[gpui::test]
    fn only_a_select_with_an_options_provider_becomes_a_choice_when_the_form_has_no_option_map(
        cx: &mut gpui::TestAppContext,
    ) {
        for (options_key, expect_choice) in [
            ("collections.twitch.rewards", true),
            ("obs.scene_names", true),
            ("vtube.model_ids", true),
            ("acme.widget_ids", true),
            (DONATION_PROVIDER_OPTIONS_KEY, true),
            ("action.ids", false),
            (forge_runtime::ENGINE_OPTIONS_KEY, false),
            (forge_runtime::VOICE_OPTIONS_PREFIX, false),
            (forge_registry::ENGINE_VOICE_OPTIONS_KEY, false),
        ] {
            let (_host, fields) = build(
                cx,
                &dynamic_select(options_key),
                &FieldConfig::new(),
                &FieldConfig::new(),
                "",
            );

            assert_eq!(
                matches!(fields.as_slice(), [ConfigField::Choice { .. }]),
                expect_choice,
                "{options_key}"
            );
        }
    }

    #[test]
    fn a_form_asks_integrations_only_for_the_choices_they_serve() {
        let choices = CollectionChoices::for_specs(&[
            dynamic_select("acme.widget_ids"),
            dynamic_select(DONATION_PROVIDER_OPTIONS_KEY),
            dynamic_select(forge_runtime::ENGINE_OPTIONS_KEY),
        ]);

        assert_eq!(choices.option_keys(), ["acme.widget_ids"]);
    }

    #[test]
    fn the_donation_service_choice_offers_any_then_every_shipped_service() {
        use forge_registry::TriggerKindDescriptor as _;

        let specs = forge_runtime::DonationReceivedDescriptor.config_fields();
        let choices = CollectionChoices::for_specs(&specs);
        let mut fields = vec![choice("provider", "", None)];

        choices.refresh(&mut fields, &ChoiceOptions::new(), Some("Any"));

        let offered = match &fields[0] {
            ConfigField::Choice { options, .. } => options.clone(),
            _ => panic!("expected a choice field"),
        };
        assert_eq!(
            offered,
            [
                ChoiceOption::new("", "Any"),
                ChoiceOption::new("donatello", "Donatello"),
                ChoiceOption::new("monobank", "monobank"),
            ]
        );
    }

    const CODE_KEY: &str = "body";
    const CODE_GATE: &str = "use_body";
    const RHAI_BODY: &str = "let n = 1;\r\nif n > 0 {\n\tprint(\"привіт 🚀\");\n}\n";
    const JSON_BODY: &str = "{\n  \"items\": [1, 2.5, null],\n  \"name\": \"ü\"\n}";

    fn code_spec(language: CodeLanguage) -> FormField {
        FormField::Code {
            key: CODE_KEY,
            label: "Body",
            language,
            kind: CodeKind::Script,
        }
    }

    fn code_editor_of(fields: &[ConfigField]) -> Entity<CodeEditor> {
        match fields {
            [ConfigField::Code { editor, .. }] => editor.clone(),
            _ => panic!("expected a single code field"),
        }
    }

    #[gpui::test]
    fn a_code_field_writes_what_its_editor_holds_as_a_verbatim_string(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::test_support::install_presentation(cx);
        for (language, stored, edited, expected) in [
            (CodeLanguage::Rhai, Some(RHAI_BODY), None, RHAI_BODY),
            (CodeLanguage::Json, Some(JSON_BODY), None, JSON_BODY),
            (CodeLanguage::Rhai, None, None, ""),
            (CodeLanguage::Json, Some(""), None, ""),
            (
                CodeLanguage::Rhai,
                Some(RHAI_BODY),
                Some("print(2);"),
                "print(2);",
            ),
            (CodeLanguage::Rhai, Some(RHAI_BODY), Some(""), ""),
        ] {
            let seeded: FieldConfig = stored
                .map(|body| (CODE_KEY.to_owned(), Variant::String(body.to_owned())))
                .into_iter()
                .collect();
            let (_host, fields) = build(cx, &code_spec(language), &FieldConfig::new(), &seeded, "");
            if let Some(text) = edited {
                let editor = code_editor_of(&fields);
                editor.update(cx, |editor, cx| editor.set_content(text, cx));
            }

            assert_eq!(
                collected(cx, &fields, &FieldConfig::new()).get(CODE_KEY),
                Some(&Variant::String(expected.to_owned())),
                "{language:?} stored {stored:?} edited {edited:?}"
            );
        }
    }

    #[gpui::test]
    fn a_gated_code_field_writes_only_while_its_toggle_is_on(cx: &mut gpui::TestAppContext) {
        crate::test_support::install_presentation(cx);
        for (toggle, expected) in [
            (true, Some(Variant::String(RHAI_BODY.to_owned()))),
            (false, None),
        ] {
            let spec = FormField::Optional {
                key: CODE_GATE,
                label: "Use body",
                inner: Box::new(code_spec(CodeLanguage::Rhai)),
            };
            let seeded = config(&[
                (CODE_GATE, Variant::Bool(toggle)),
                (CODE_KEY, Variant::String(RHAI_BODY.to_owned())),
            ]);
            let (_host, fields) = build(cx, &spec, &FieldConfig::new(), &seeded, "");

            assert_eq!(
                collected(cx, &fields, &FieldConfig::new()).get(CODE_KEY),
                expected.as_ref(),
                "toggle {toggle}"
            );
        }
    }

    struct CommitLog(Vec<String>);

    fn log_commit(log: &mut CommitLog, event: &InputEvent, _: &mut Context<CommitLog>) {
        let label = match event {
            InputEvent::Changed(_) => "changed",
            InputEvent::Submitted(_) => "submitted",
            InputEvent::Blurred(_) => "blurred",
            InputEvent::Cancelled => "cancelled",
        };
        log.0.push(label.to_owned());
    }

    #[gpui::test]
    fn a_code_field_hands_only_a_blur_commit_to_the_form(cx: &mut gpui::TestAppContext) {
        crate::test_support::install_presentation(cx);
        let host = cx.update(|cx| cx.new(|_| CommitLog(Vec::new())));
        let fields = host.update(cx, |_, cx| {
            let palette = forge_components::ThemeId::ForgeDefault.palette();
            let ctx = FoldContext {
                kind_id: "",
                config: &FieldConfig::new(),
                defaults: &FieldConfig::new(),
                palette: &palette,
                capabilities: FieldCapabilities::inline(ChoiceSupport::Text),
                on_committed: log_commit as ConfigCommitHandler<CommitLog>,
            };
            let mut fields = Vec::new();
            fold_config_field(&code_spec(CodeLanguage::Rhai), None, &ctx, &mut fields, cx);
            fields
        });
        let editor = code_editor_of(&fields);

        editor.update(cx, |_, cx| {
            cx.emit(InputEvent::Changed("a".into()));
            cx.emit(InputEvent::Submitted("a".into()));
            cx.emit(InputEvent::Cancelled);
            cx.emit(InputEvent::Blurred("a".into()));
        });
        cx.run_until_parked();

        assert_eq!(host.read_with(cx, |log, _| log.0.clone()), ["blurred"]);
    }

    const AMOUNT_KEY: &str = "amount";
    const AMOUNT_UNIT_KEY: &str = "amount_unit";
    const AMOUNT_GATE: &str = "use_amount";
    const AMOUNT_BOUNDS: UnitAmountBounds = UnitAmountBounds {
        min: 1,
        max_base_units: 120,
        units: &[
            forge_registry::AmountUnit {
                value: "seconds",
                base_units: 1,
            },
            forge_registry::AmountUnit {
                value: "minutes",
                base_units: 60,
            },
        ],
    };

    fn unit_amount_spec() -> FormField {
        FormField::UnitAmount {
            key: AMOUNT_KEY,
            label: "Amount",
            unit_key: AMOUNT_UNIT_KEY,
            bounds: AMOUNT_BOUNDS,
        }
    }

    fn stored_unit(unit: Option<&str>) -> FieldConfig {
        unit.map(|unit| (AMOUNT_UNIT_KEY.to_owned(), Variant::String(unit.to_owned())))
            .into_iter()
            .collect()
    }

    fn amount_input(fields: &[ConfigField]) -> Entity<TextInput> {
        fields
            .iter()
            .find_map(|field| match field {
                ConfigField::UnitAmount { input, .. } => Some(input.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("a unit amount spec builds a unit amount field"))
    }

    fn type_amount(cx: &mut gpui::TestAppContext, fields: &[ConfigField], typed: &str) {
        amount_input(fields).update(cx, |input, cx| input.set_content(typed.to_owned(), cx));
    }

    fn shown_amount(cx: &mut gpui::TestAppContext, fields: &[ConfigField]) -> String {
        amount_input(fields).read_with(cx, |input, _| input.content().to_owned())
    }

    #[gpui::test]
    fn a_unit_amount_saves_its_unit_and_the_amount_clamped_into_that_units_range(
        cx: &mut gpui::TestAppContext,
    ) {
        for (unit, typed, saved_unit, saved_amount) in [
            (Some("seconds"), "500", "seconds", 120),
            (Some("seconds"), "120", "seconds", 120),
            (Some("minutes"), "500", "minutes", 2),
            (Some("minutes"), "0", "minutes", 1),
            (Some("weeks"), "60", "seconds", 60),
            (None, "60", "seconds", 60),
        ] {
            let seeded = stored_unit(unit);
            let (_host, fields) = build(cx, &unit_amount_spec(), &FieldConfig::new(), &seeded, "");
            type_amount(cx, &fields, typed);

            let saved = collected(cx, &fields, &FieldConfig::new());
            assert_eq!(
                (saved.get(AMOUNT_UNIT_KEY), saved.get(AMOUNT_KEY)),
                (
                    Some(&Variant::String(saved_unit.to_owned())),
                    Some(&Variant::Int(saved_amount))
                ),
                "stored unit {unit:?}, typed {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn picking_a_larger_unit_snaps_the_shown_amount_into_the_new_range(
        cx: &mut gpui::TestAppContext,
    ) {
        let seeded = stored_unit(Some("seconds"));
        let (_host, mut fields) = build(cx, &unit_amount_spec(), &FieldConfig::new(), &seeded, "");
        type_amount(cx, &fields, "100");

        cx.update(|cx| set_picked_value(&mut fields, AMOUNT_UNIT_KEY, "minutes", cx));

        assert_eq!(shown_amount(cx, &fields), "2");
    }

    #[gpui::test]
    fn leaving_the_amount_after_a_unit_switch_snaps_it_to_the_new_units_bound(
        cx: &mut gpui::TestAppContext,
    ) {
        let seeded = stored_unit(Some("seconds"));
        let (_host, mut fields) = build(cx, &unit_amount_spec(), &FieldConfig::new(), &seeded, "");
        cx.update(|cx| set_picked_value(&mut fields, AMOUNT_UNIT_KEY, "minutes", cx));
        type_amount(cx, &fields, "50");

        amount_input(&fields).update(cx, |_, cx| cx.emit(InputEvent::Blurred("50".into())));
        cx.run_until_parked();

        assert_eq!(shown_amount(cx, &fields), "2");
    }

    #[gpui::test]
    fn a_gated_off_unit_amount_writes_neither_its_amount_nor_its_unit(
        cx: &mut gpui::TestAppContext,
    ) {
        let spec = FormField::Optional {
            key: AMOUNT_GATE,
            label: "Use amount",
            inner: Box::new(unit_amount_spec()),
        };
        let seeded = config(&[(AMOUNT_GATE, Variant::Bool(false))]);
        let (_host, fields) = build(cx, &spec, &FieldConfig::new(), &seeded, "");
        type_amount(cx, &fields, "30");

        assert_eq!(
            collected(cx, &fields, &FieldConfig::new()),
            config(&[(AMOUNT_GATE, Variant::Bool(false))])
        );
    }

    const DURATION_KEY: &str = "timeout_ms";

    fn duration_spec(min_ms: i64, max_ms: i64) -> FormField {
        FormField::Duration {
            key: DURATION_KEY,
            label: "Timeout",
            bounds: forge_registry::DurationBounds::in_millis(min_ms, max_ms),
        }
    }

    fn duration_unit_picker() -> String {
        AmountScale::of(&duration_spec(100, 600_000))
            .map(|scale| scale.unit_picker_key(DURATION_KEY))
            .unwrap_or_else(|| panic!("a duration spec has an amount scale"))
    }

    fn stored_millis(millis: i64) -> FieldConfig {
        config(&[(DURATION_KEY, Variant::Int(millis))])
    }

    #[gpui::test]
    fn a_duration_saves_millis_clamped_into_the_shown_units_range(cx: &mut gpui::TestAppContext) {
        for (stored, typed, saved) in [
            (30_000, "45", 45_000),
            (30_000, "900", 600_000),
            (30_000, "0", 1_000),
            (120_000, "3", 180_000),
            (120_000, "11", 600_000),
            (500, "50", 100),
            (500, "600000", 600_000),
        ] {
            let (_host, fields) = build(
                cx,
                &duration_spec(100, 600_000),
                &FieldConfig::new(),
                &stored_millis(stored),
                "",
            );
            type_amount(cx, &fields, typed);

            assert_eq!(
                collected(cx, &fields, &FieldConfig::new()),
                stored_millis(saved),
                "stored {stored} ms, typed {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn an_unedited_duration_saves_the_stored_amount_it_loaded(cx: &mut gpui::TestAppContext) {
        let in_millis = forge_registry::DurationBounds::in_millis(100, 600_000);
        let in_seconds = forge_registry::DurationBounds::in_seconds(1, 1_209_600);
        let in_minutes = forge_registry::DurationBounds::in_minutes(1, 1_440);
        for (bounds, stored) in [
            (in_millis, 100),
            (in_millis, 1_500),
            (in_millis, 30_000),
            (in_millis, 90_000),
            (in_millis, 120_000),
            (in_millis, 600_000),
            (in_seconds, 1),
            (in_seconds, 90),
            (in_seconds, 600),
            (in_seconds, 86_400),
            (in_seconds, 1_209_600),
            (in_minutes, 1),
            (in_minutes, 30),
            (in_minutes, 90),
            (in_minutes, 1_440),
        ] {
            let spec = FormField::Duration {
                key: DURATION_KEY,
                label: "Timeout",
                bounds,
            };
            let (_host, fields) = build(cx, &spec, &FieldConfig::new(), &stored_millis(stored), "");

            assert_eq!(
                collected(cx, &fields, &FieldConfig::new()),
                stored_millis(stored),
                "stored {stored} in {bounds:?}"
            );
        }
    }

    const TIMER_INTERVAL_KEY: &str = "interval_minutes";

    fn timer_interval_spec() -> FormField {
        use forge_registry::TriggerKindDescriptor;
        forge_runtime::TimerTickDescriptor
            .config_fields()
            .into_iter()
            .find(|field| field.key() == TIMER_INTERVAL_KEY)
            .unwrap_or_else(|| panic!("the timer trigger declares an interval field"))
    }

    fn timer_interval_unit_picker() -> String {
        AmountScale::of(&timer_interval_spec())
            .map(|scale| scale.unit_picker_key(TIMER_INTERVAL_KEY))
            .unwrap_or_else(|| panic!("the timer interval has an amount scale"))
    }

    #[gpui::test]
    fn a_timer_interval_switched_to_hours_shows_hours_and_still_saves_minutes(
        cx: &mut gpui::TestAppContext,
    ) {
        let stored = config(&[(TIMER_INTERVAL_KEY, Variant::Int(1_440))]);
        let (_host, mut fields) =
            build(cx, &timer_interval_spec(), &FieldConfig::new(), &stored, "");

        cx.update(|cx| {
            set_picked_value(
                &mut fields,
                &timer_interval_unit_picker(),
                forge_registry::DURATION_UNIT_HOURS,
                cx,
            )
        });

        assert_eq!(
            (
                shown_amount(cx, &fields),
                collected(cx, &fields, &FieldConfig::new())
            ),
            ("24".to_owned(), stored)
        );
    }

    #[gpui::test]
    fn picking_another_duration_unit_converts_the_shown_amount(cx: &mut gpui::TestAppContext) {
        for (to, shown) in [
            (forge_registry::DURATION_UNIT_MILLISECONDS, "90000"),
            (forge_registry::DURATION_UNIT_MINUTES, "2"),
        ] {
            let (_host, mut fields) = build(
                cx,
                &duration_spec(100, 600_000),
                &FieldConfig::new(),
                &stored_millis(90_000),
                "",
            );

            cx.update(|cx| set_picked_value(&mut fields, &duration_unit_picker(), to, cx));

            assert_eq!(shown_amount(cx, &fields), shown, "90 seconds into {to}");
        }
    }

    #[gpui::test]
    fn a_duration_rounded_to_zero_by_a_unit_switch_settles_at_the_new_units_minimum(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_host, mut fields) = build(
            cx,
            &duration_spec(100, 30_000),
            &FieldConfig::new(),
            &stored_millis(100),
            "",
        );

        cx.update(|cx| {
            set_picked_value(
                &mut fields,
                &duration_unit_picker(),
                forge_registry::DURATION_UNIT_SECONDS,
                cx,
            )
        });

        assert_eq!(shown_amount(cx, &fields), "1");
    }

    #[gpui::test]
    fn switching_the_unit_then_typing_saves_the_amount_in_the_new_unit(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_host, mut fields) = build(
            cx,
            &duration_spec(100, 600_000),
            &FieldConfig::new(),
            &stored_millis(30_000),
            "",
        );
        cx.update(|cx| {
            set_picked_value(
                &mut fields,
                &duration_unit_picker(),
                forge_registry::DURATION_UNIT_MINUTES,
                cx,
            )
        });
        type_amount(cx, &fields, "2");

        assert_eq!(
            collected(cx, &fields, &FieldConfig::new()),
            stored_millis(120_000)
        );
    }

    const CHANNEL_KEY: &str = "channel";
    const CHANNELS: i64 = 16;

    fn channel_spec() -> FormField {
        FormField::OneBasedInteger {
            key: CHANNEL_KEY,
            label: "Channel",
            count: CHANNELS,
        }
    }

    fn optional_channel_spec() -> FormField {
        FormField::Optional {
            key: CHANNEL_KEY,
            label: "Channel",
            inner: Box::new(channel_spec()),
        }
    }

    fn input_text(cx: &mut gpui::TestAppContext, fields: &[ConfigField]) -> Option<String> {
        cx.update(|cx| {
            fields.iter().find_map(|field| match field {
                ConfigField::Input { input, .. } => Some(input.read(cx).content().to_owned()),
                _ => None,
            })
        })
    }

    #[gpui::test]
    fn a_one_based_field_shows_the_stored_value_plus_one(cx: &mut gpui::TestAppContext) {
        for (stored, shown) in [(0, "1"), (15, "16")] {
            let seeded = config(&[(CHANNEL_KEY, Variant::Int(stored))]);

            let (_host, fields) = fold_untouched(cx, &channel_spec(), &FieldConfig::new(), &seeded);

            assert_eq!(input_text(cx, &fields).as_deref(), Some(shown));
        }
    }

    #[gpui::test]
    fn a_one_based_field_saves_the_typed_value_minus_one_inside_its_bounds(
        cx: &mut gpui::TestAppContext,
    ) {
        for (typed, saved) in [("1", 0), ("16", 15), ("17", 15), ("0", 0), ("-4", 0)] {
            let seeded = config(&[(CHANNEL_KEY, Variant::Int(SEEDED))]);

            let (_host, fields) = build(cx, &channel_spec(), &FieldConfig::new(), &seeded, typed);

            assert_eq!(
                collected(cx, &fields, &seeded).get(CHANNEL_KEY),
                Some(&Variant::Int(saved)),
                "typed {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn an_untouched_one_based_field_saves_what_was_stored(cx: &mut gpui::TestAppContext) {
        for spec in [channel_spec(), optional_channel_spec()] {
            let seeded = config(&[(CHANNEL_KEY, Variant::Int(9))]);

            let (_host, fields) = fold_untouched(cx, &spec, &FieldConfig::new(), &seeded);

            assert_eq!(collected(cx, &fields, &seeded), seeded, "{spec:?}");
        }
    }

    #[gpui::test]
    fn a_blank_optional_one_based_field_stores_no_value(cx: &mut gpui::TestAppContext) {
        let seeded = config(&[(CHANNEL_KEY, Variant::Int(2))]);

        let (_host, fields) = build(cx, &channel_spec(), &FieldConfig::new(), &seeded, "");

        assert_eq!(collected(cx, &fields, &seeded).get(CHANNEL_KEY), None);
    }
}
