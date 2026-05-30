use std::sync::Arc;

use forge_registry::TriggerCategory as RegistryCategory;
use forge_types::{ActionId, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant};
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_SM, FONT_XS, Spacing, sp, spf};
use iced::{Alignment, Background, Border, Element, Length, Task};

use crate::Message;
use crate::message::ActionsMsg;
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone)]
pub struct TriggerPickerState {
    pub action_id: ActionId,
    pub next_position: i64,
    pub level1: Option<PlatformGroup>,
    pub level2: Option<TriggerSubGroup>,
    pub available_instances: Vec<TriggerPickerEntry>,
    pub is_loading: bool,
}

#[derive(Debug, Clone)]
pub struct TriggerPickerEntry {
    pub kind_id: String,
    pub label: String,
    pub sub_group_label: String,
    pub default_instance_id: TriggerInstanceId,
    pub custom_instances: Vec<CustomInstanceChip>,
}

#[derive(Debug, Clone)]
pub struct CustomInstanceChip {
    pub id: TriggerInstanceId,
    pub name: String,
    pub override_summary: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlatformGroup {
    Twitch,
    Obs,
    Script,
    Core,
}

#[derive(Clone, Debug)]
pub struct TriggerSubGroup {
    pub label: String,
    pub kind_id_prefix: String,
}

impl PartialEq for TriggerSubGroup {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
    }
}

impl Eq for TriggerSubGroup {}

#[derive(Debug, Clone)]
pub enum TriggerPickerMsg {
    InstancesLoaded(Result<Vec<TriggerPickerEntry>, String>),
    Level1Selected(PlatformGroup),
    Level2Selected(TriggerSubGroup),
    DefaultSelected(TriggerInstanceId),
    CustomSelected(TriggerInstanceId),
    Cancelled,
}

pub(crate) fn category_display_label(cat: RegistryCategory) -> &'static str {
    match cat {
        RegistryCategory::Chat => "Chat",
        RegistryCategory::Subscriptions => "Subscriptions",
        RegistryCategory::Bits => "Bits",
        RegistryCategory::Raids => "Raids",
        RegistryCategory::ChannelPoints => "Channel Points",
        RegistryCategory::Polls => "Polls",
        RegistryCategory::Predictions => "Predictions",
        RegistryCategory::Hype => "Hype Train",
        RegistryCategory::Charity => "Charity",
        RegistryCategory::Goals => "Goals",
        RegistryCategory::Clips => "Clips",
        RegistryCategory::Streams => "Streams",
        RegistryCategory::Users => "Users",
        RegistryCategory::Obs => "Scenes",
        RegistryCategory::VTube => "VTube Studio",
        RegistryCategory::Discord => "Discord",
        RegistryCategory::Midi => "MIDI",
        RegistryCategory::Hotkey => "Hotkeys",
        RegistryCategory::Core => "Core",
        RegistryCategory::Server => "Server Events",
        RegistryCategory::Timer => "Timers",
        RegistryCategory::Ungrouped => "Other",
    }
}

pub fn build_picker_entries(
    descriptor_infos: Vec<(String, String, String)>,
    all_instances: Vec<TriggerInstance>,
) -> Vec<TriggerPickerEntry> {
    descriptor_infos
        .into_iter()
        .filter_map(|(kind_id, label, sub_group_label)| {
            let default_inst = all_instances
                .iter()
                .find(|i| i.kind_id == kind_id && !i.user_defined)?;
            let custom_instances = all_instances
                .iter()
                .filter(|i| i.kind_id == kind_id && i.user_defined)
                .map(|i| CustomInstanceChip {
                    id: i.id,
                    name: i.name.clone(),
                    override_summary: format_override_summary(&i.overrides),
                })
                .collect();
            Some(TriggerPickerEntry {
                kind_id,
                label,
                sub_group_label,
                default_instance_id: default_inst.id,
                custom_instances,
            })
        })
        .collect()
}

fn platform_group_for(kind_id: &str) -> PlatformGroup {
    if kind_id.starts_with("twitch.") {
        PlatformGroup::Twitch
    } else if kind_id.starts_with("obs.") {
        PlatformGroup::Obs
    } else if kind_id.starts_with("script.") {
        PlatformGroup::Script
    } else {
        PlatformGroup::Core
    }
}

fn format_override_summary(overrides: &TriggerConfig) -> String {
    if overrides.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = overrides
        .iter()
        .map(|(k, v)| format!("{}={}", k, variant_short(v)))
        .collect();
    let full = parts.join(", ");
    if full.len() > 40 {
        format!("{}\u{2026}", &full[..37])
    } else {
        full
    }
}

fn variant_short(v: &Variant) -> String {
    match v {
        Variant::String(s) => s.clone(),
        Variant::Int(n) => n.to_string(),
        Variant::Float(f) => format!("{:.1}", f),
        Variant::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Variant::Array(_) => "[...]".to_string(),
        Variant::Object(_) => "{...}".to_string(),
        Variant::Datetime(dt) => dt.to_string(),
    }
}

pub fn update(
    state: &mut Option<TriggerPickerState>,
    rt: &RuntimeView,
    msg: TriggerPickerMsg,
) -> Task<Message> {
    match msg {
        TriggerPickerMsg::InstancesLoaded(Ok(entries)) => {
            if let Some(s) = state.as_mut() {
                s.available_instances = entries;
                s.is_loading = false;
            }
            Task::none()
        }
        TriggerPickerMsg::InstancesLoaded(Err(e)) => {
            if let Some(s) = state.as_mut() {
                s.is_loading = false;
            }
            tracing::warn!(error = %e, "trigger picker load failed");
            Task::none()
        }
        TriggerPickerMsg::Level1Selected(group) => {
            if let Some(s) = state.as_mut() {
                s.level1 = Some(group);
                s.level2 = None;
            }
            Task::none()
        }
        TriggerPickerMsg::Level2Selected(subgroup) => {
            if let Some(s) = state.as_mut() {
                s.level2 = Some(subgroup);
            }
            Task::none()
        }
        TriggerPickerMsg::DefaultSelected(instance_id)
        | TriggerPickerMsg::CustomSelected(instance_id) => {
            let Some(s) = state.as_ref() else {
                return Task::none();
            };
            let action_id = s.action_id;
            let position = s.next_position;
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.trigger_instance_repo()
                        .link_action(action_id, instance_id, position)
                        .await
                        .map(|_| action_id)
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::TriggerInstanceAssigned(r)),
            )
        }
        TriggerPickerMsg::Cancelled => {
            *state = None;
            Task::none()
        }
    }
}

pub fn view<'a>(
    picker: &'a TriggerPickerState,
    _rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let on_cancel = Message::Actions(ActionsMsg::TriggerPickerMsg(TriggerPickerMsg::Cancelled));

    if picker.is_loading {
        let loading_body = container(
            text("Loading triggers\u{2026}")
                .size(FONT_SM)
                .color(p.text_muted),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([sp(Spacing::Lg), sp(Spacing::Lg)]);

        let cancel_btn = forge_widgets::secondary_button("Cancel", on_cancel.clone(), palette);
        let footer_bar = container(
            row![iced::widget::Space::new().width(Length::Fill), cancel_btn]
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([12_u16, 16_u16])
        .style(move |_t: &iced::Theme| iced::widget::container::Style {
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

        let content = column![loading_body, footer_bar]
            .width(Length::Fill)
            .height(Length::Fill);

        return forge_widgets::SideSheet::new(content)
            .open(true)
            .palette(palette)
            .width(forge_widgets::SheetWidth::new(560.0, 400.0, 800.0))
            .header(forge_widgets::SheetHeader {
                title: std::borrow::Cow::Borrowed("Add trigger"),
                subtitle: None,
                on_close: Some(on_cancel.clone()),
            })
            .on_close(on_cancel)
            .into();
    }

    let all_groups = [
        PlatformGroup::Twitch,
        PlatformGroup::Obs,
        PlatformGroup::Script,
        PlatformGroup::Core,
    ];
    let platform_group_label = |g: PlatformGroup| match g {
        PlatformGroup::Twitch => "Twitch",
        PlatformGroup::Obs => "OBS",
        PlatformGroup::Script => "Script",
        PlatformGroup::Core => "Core",
    };
    let platform_group_color = |g: PlatformGroup| match g {
        PlatformGroup::Twitch => p.brand,
        PlatformGroup::Obs => p.text_secondary,
        PlatformGroup::Script => p.warning,
        PlatformGroup::Core => p.info,
    };

    let mut platform_col: iced::widget::Column<'_, Message> = column![]
        .spacing(spf(Spacing::Xxs))
        .padding([sp(Spacing::Sm), sp(Spacing::Xs)]);

    for &group in &all_groups {
        let selected = picker.level1 == Some(group);
        let label = platform_group_label(group);
        let color = platform_group_color(group);
        let dot = container(iced::widget::Space::new().width(6.0).height(6.0))
            .width(6.0)
            .height(6.0)
            .style(move |_t: &iced::Theme| iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 3.0.into(),
                    ..Border::default()
                },
                ..iced::widget::container::Style::default()
            });

        let label_text = text(label).size(FONT_SM).color(if selected {
            p.text_primary
        } else {
            p.text_secondary
        });

        let inner = row![dot, label_text]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center);

        let bg = if selected {
            Some(Background::Color(p.surface_overlay))
        } else {
            None
        };
        let btn = iced::widget::button(inner)
            .on_press(Message::Actions(ActionsMsg::TriggerPickerMsg(
                TriggerPickerMsg::Level1Selected(group),
            )))
            .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
            .width(Length::Fill)
            .style(move |_t: &iced::Theme, _s| iced::widget::button::Style {
                background: bg,
                text_color: p.text_secondary,
                border: Border {
                    color: if selected {
                        p.brand
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: if selected { 1.0 } else { 0.0 },
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            });

        platform_col = platform_col.push(btn);
    }

    let platform_entries: Vec<&TriggerPickerEntry> = picker
        .available_instances
        .iter()
        .filter(|e| {
            picker
                .level1
                .map(|g| platform_group_for(&e.kind_id) == g)
                .unwrap_or(true)
        })
        .collect();

    let mut subgroups_seen: std::collections::LinkedList<String> =
        std::collections::LinkedList::new();
    let mut subgroups: Vec<String> = Vec::new();
    for entry in &platform_entries {
        if !subgroups_seen.contains(&entry.sub_group_label) {
            subgroups_seen.push_back(entry.sub_group_label.clone());
            subgroups.push(entry.sub_group_label.clone());
        }
    }

    let mut subgroup_col: iced::widget::Column<'_, Message> = column![]
        .spacing(spf(Spacing::Xxs))
        .padding([sp(Spacing::Sm), sp(Spacing::Xs)]);

    if picker.level1.is_none() {
        subgroup_col = subgroup_col.push(
            text("Select a platform")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono),
        );
    } else if subgroups.is_empty() {
        subgroup_col = subgroup_col.push(
            text("No triggers available")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono),
        );
    } else {
        for label in subgroups {
            let selected = picker
                .level2
                .as_ref()
                .map(|sg| sg.label == label)
                .unwrap_or(false);
            let label_clone = label.clone();
            let text_color = if selected {
                p.text_primary
            } else {
                p.text_secondary
            };
            let bg = if selected {
                Some(Background::Color(p.surface_overlay))
            } else {
                None
            };
            let btn = iced::widget::button(text(label.clone()).size(FONT_SM).color(text_color))
                .on_press(Message::Actions(ActionsMsg::TriggerPickerMsg(
                    TriggerPickerMsg::Level2Selected(TriggerSubGroup {
                        label: label_clone,
                        kind_id_prefix: String::new(),
                    }),
                )))
                .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
                .width(Length::Fill)
                .style(move |_t: &iced::Theme, _s| iced::widget::button::Style {
                    background: bg,
                    text_color: p.text_secondary,
                    border: Border {
                        color: if selected {
                            p.brand
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        width: if selected { 1.0 } else { 0.0 },
                        radius: 4.0.into(),
                    },
                    shadow: iced::Shadow::default(),
                    snap: false,
                });
            subgroup_col = subgroup_col.push(btn);
        }
    }

    let visible_entries: Vec<&TriggerPickerEntry> = platform_entries
        .into_iter()
        .filter(|e| {
            picker
                .level2
                .as_ref()
                .map(|sg| sg.label == e.sub_group_label)
                .unwrap_or(true)
        })
        .collect();

    let mut trigger_list: iced::widget::Column<'_, Message> = column![]
        .spacing(spf(Spacing::Xxs))
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

    if picker.level1.is_none() {
        trigger_list = trigger_list.push(
            text("Select a platform to browse triggers.")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono),
        );
    } else if visible_entries.is_empty() {
        trigger_list = trigger_list.push(
            text("No triggers available for this selection.")
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono),
        );
    } else {
        for entry in visible_entries {
            let default_id = entry.default_instance_id;
            let header_inner = row![
                tabler_icon(Icon::PlayerPlay, FONT_XS, p.brand),
                text(entry.label.clone())
                    .size(FONT_SM)
                    .color(p.text_primary),
                iced::widget::Space::new().width(Length::Fill),
                text("(default)")
                    .size(FONT_XS)
                    .color(p.text_faint)
                    .font(mono),
            ]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center);

            let default_btn = iced::widget::button(header_inner)
                .on_press(Message::Actions(ActionsMsg::TriggerPickerMsg(
                    TriggerPickerMsg::DefaultSelected(default_id),
                )))
                .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
                .width(Length::Fill)
                .style(
                    move |_t: &iced::Theme, status| iced::widget::button::Style {
                        background: match status {
                            iced::widget::button::Status::Hovered
                            | iced::widget::button::Status::Pressed => {
                                Some(Background::Color(p.surface_overlay))
                            }
                            _ => None,
                        },
                        text_color: p.text_primary,
                        border: Border {
                            color: p.border_regular,
                            width: 0.5,
                            radius: 4.0.into(),
                        },
                        shadow: iced::Shadow::default(),
                        snap: false,
                    },
                );

            trigger_list = trigger_list.push(default_btn);

            for chip in &entry.custom_instances {
                let chip_id = chip.id;
                let summary = chip.override_summary.clone();
                let chip_inner = row![
                    iced::widget::Space::new().width(16.0),
                    tabler_icon(Icon::Plus, FONT_XS, p.success),
                    text(chip.name.clone())
                        .size(FONT_SM)
                        .color(p.text_secondary),
                    iced::widget::Space::new().width(Length::Fill),
                    text(summary).size(FONT_XS).color(p.text_muted).font(mono),
                ]
                .spacing(spf(Spacing::Xs))
                .align_y(Alignment::Center);

                let chip_btn = iced::widget::button(chip_inner)
                    .on_press(Message::Actions(ActionsMsg::TriggerPickerMsg(
                        TriggerPickerMsg::CustomSelected(chip_id),
                    )))
                    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
                    .width(Length::Fill)
                    .style(
                        move |_t: &iced::Theme, status| iced::widget::button::Style {
                            background: match status {
                                iced::widget::button::Status::Hovered
                                | iced::widget::button::Status::Pressed => {
                                    Some(Background::Color(p.surface_overlay))
                                }
                                _ => None,
                            },
                            text_color: p.text_secondary,
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                            snap: false,
                        },
                    );
                trigger_list = trigger_list.push(chip_btn);
            }
        }
    }

    let trigger_col = scrollable(trigger_list).height(Length::Fill);

    let vdivider = |_: ()| {
        container(iced::widget::Space::new().width(0.5).height(Length::Fill))
            .width(0.5)
            .height(Length::Fill)
            .style(move |_t: &iced::Theme| iced::widget::container::Style {
                background: Some(Background::Color(p.border_regular)),
                ..iced::widget::container::Style::default()
            })
    };

    let three_cols = row![
        container(scrollable(platform_col))
            .width(140)
            .height(Length::Fill),
        vdivider(()),
        container(scrollable(subgroup_col))
            .width(160)
            .height(Length::Fill),
        vdivider(()),
        container(trigger_col)
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .spacing(0)
    .height(Length::Fill);

    let cancel_btn = forge_widgets::secondary_button("Cancel", on_cancel.clone(), palette);

    let footer_bar = container(
        row![iced::widget::Space::new().width(Length::Fill), cancel_btn].align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([12_u16, 16_u16])
    .style(move |_t: &iced::Theme| iced::widget::container::Style {
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    });

    let body = container(three_cols)
        .width(Length::Fill)
        .height(Length::Fill);

    let content = column![body, footer_bar]
        .width(Length::Fill)
        .height(Length::Fill);

    forge_widgets::SideSheet::new(content)
        .open(true)
        .palette(palette)
        .width(forge_widgets::SheetWidth::new(560.0, 400.0, 800.0))
        .header(forge_widgets::SheetHeader {
            title: std::borrow::Cow::Borrowed("Add trigger"),
            subtitle: None,
            on_close: Some(on_cancel.clone()),
        })
        .on_close(on_cancel)
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::{ActionId, TriggerInstance, TriggerInstanceId};

    #[test]
    fn picker_open_sets_loading_state() {
        let state = TriggerPickerState {
            action_id: ActionId::new(),
            next_position: 0,
            level1: None,
            level2: None,
            available_instances: Vec::new(),
            is_loading: true,
        };
        assert!(state.is_loading);
        assert!(state.level1.is_none());
        assert!(state.level2.is_none());
        assert!(state.available_instances.is_empty());
    }

    #[test]
    fn picker_cancelled_clears_state() {
        let mut state: Option<TriggerPickerState> = Some(TriggerPickerState {
            action_id: ActionId::new(),
            next_position: 0,
            level1: Some(PlatformGroup::Twitch),
            level2: None,
            available_instances: Vec::new(),
            is_loading: false,
        });
        assert!(state.is_some());
        state = None;
        assert!(state.is_none());
    }

    #[test]
    fn build_entries_skips_kind_without_default_instance() {
        let descriptor_infos = vec![(
            "twitch.chat.command".to_owned(),
            "Chat Command".to_owned(),
            "Chat".to_owned(),
        )];
        let entries = build_picker_entries(descriptor_infos, vec![]);
        assert!(entries.is_empty());
    }

    #[test]
    fn build_entries_includes_custom_instances() {
        let default_id = TriggerInstanceId::new();
        let custom_id = TriggerInstanceId::new();
        let descriptor_infos = vec![(
            "twitch.support.subscriber".to_owned(),
            "New Subscriber".to_owned(),
            "Subscriptions".to_owned(),
        )];
        let instances = vec![
            TriggerInstance {
                id: default_id,
                kind_id: "twitch.support.subscriber".to_owned(),
                name: "New Subscriber (default)".to_owned(),
                overrides: Default::default(),
                enabled: true,
                user_defined: false,
                platform_scope: Default::default(),
            },
            TriggerInstance {
                id: custom_id,
                kind_id: "twitch.support.subscriber".to_owned(),
                name: "VIP Sub Alert".to_owned(),
                overrides: Default::default(),
                enabled: true,
                user_defined: true,
                platform_scope: Default::default(),
            },
        ];
        let entries = build_picker_entries(descriptor_infos, instances);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].default_instance_id, default_id);
        assert_eq!(entries[0].custom_instances.len(), 1);
        assert_eq!(entries[0].custom_instances[0].name, "VIP Sub Alert");
    }

    #[test]
    fn format_override_summary_empty_map_yields_empty_string() {
        let overrides = std::collections::BTreeMap::new();
        assert_eq!(format_override_summary(&overrides), "");
    }

    #[test]
    fn format_override_summary_truncates_long_output() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert(
            "very_long_key_name".to_owned(),
            Variant::String("some_very_long_value_that_exceeds_forty_chars".to_owned()),
        );
        let result = format_override_summary(&overrides);
        assert!(result.len() <= 41);
    }

    #[test]
    fn platform_group_twitch_prefix() {
        assert_eq!(
            platform_group_for("twitch.chat.command"),
            PlatformGroup::Twitch
        );
    }

    #[test]
    fn platform_group_obs_prefix() {
        assert_eq!(
            platform_group_for("obs.scenes.current_changed"),
            PlatformGroup::Obs
        );
    }

    #[test]
    fn platform_group_script_prefix() {
        assert_eq!(
            platform_group_for("script.event.custom"),
            PlatformGroup::Script
        );
    }

    #[test]
    fn platform_group_core_fallback() {
        assert_eq!(platform_group_for("core.logic.wait"), PlatformGroup::Core);
    }
}
