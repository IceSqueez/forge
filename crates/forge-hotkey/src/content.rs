use forge_platform_core::{
    BuiltinContent, ContentList, ContentListItem, DetailSection, SectionIcon, TrailingToken,
};

use crate::client::HotkeyClient;

impl BuiltinContent for HotkeyClient {
    fn sections(&self) -> Vec<DetailSection> {
        let snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
        let recent_triggers: Vec<_> = snap.recent_triggers.iter().rev().cloned().collect();
        drop(snap);

        let registered = self
            .id_to_combo
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let hotkey_items: Vec<ContentListItem> = registered
            .iter()
            .map(|combo| ContentListItem {
                icon: SectionIcon::new("keyboard"),
                icon_tint: None,
                name: combo.as_str().to_owned(),
                monospace_name: true,
                active: true,
                active_label: Some("ACTIVE".to_owned()),
                trailing: vec![],
                enabled: true,
            })
            .collect();

        let hotkey_count = hotkey_items.len().to_string();

        let trigger_items: Vec<ContentListItem> = recent_triggers
            .iter()
            .map(|record| {
                let time_str = record
                    .at
                    .format(
                        &time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
                            .unwrap_or_default(),
                    )
                    .unwrap_or_else(|_| "--:--:--".to_owned());
                ContentListItem {
                    icon: SectionIcon::new("bolt"),
                    icon_tint: None,
                    name: record.combo.clone(),
                    monospace_name: true,
                    active: true,
                    active_label: None,
                    trailing: vec![TrailingToken::Label(time_str)],
                    enabled: true,
                }
            })
            .collect();

        let trigger_count = trigger_items.len().to_string();

        vec![DetailSection::TwoColumnLists {
            left: Box::new(ContentList {
                title: "Registered Hotkeys".to_owned(),
                icon: SectionIcon::new("keyboard"),
                inline_label: None,
                count_label: Some(hotkey_count),
                visible_rows: None,
                row_padding_y_px: 7,
                refreshable: false,
                items: hotkey_items,
                footer: None,
            }),
            right: Box::new(ContentList {
                title: "Recent Triggers".to_owned(),
                icon: SectionIcon::new("bolt"),
                inline_label: None,
                count_label: Some(trigger_count),
                visible_rows: None,
                row_padding_y_px: 7,
                refreshable: false,
                items: trigger_items,
                footer: None,
            }),
        }]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::{BuiltinContent, DetailSection};

    use crate::client::HotkeyClient;
    use crate::combo::HotkeyCombo;

    #[test]
    fn sections_returns_one_two_column_section() {
        let c = HotkeyClient::new_for_test(None);
        let content: &dyn BuiltinContent = &*c;
        let sections = content.sections();
        assert_eq!(sections.len(), 1);
        assert!(matches!(sections[0], DetailSection::TwoColumnLists { .. }));
    }

    #[tokio::test]
    async fn registered_hotkey_appears_in_left_list() {
        let c = HotkeyClient::new_for_test(Some(true));
        let combo = HotkeyCombo::parse("Ctrl+H").unwrap();
        c.register(combo).await.unwrap();
        let content: &dyn BuiltinContent = &*c;
        let sections = content.sections();
        let DetailSection::TwoColumnLists { left, .. } = &sections[0] else {
            unreachable!("section 0 must be TwoColumnLists");
        };
        assert!(left.items.iter().any(|item| item.name == "Ctrl+H"));
    }

    #[test]
    fn recent_triggers_initially_empty() {
        let c = HotkeyClient::new_for_test(None);
        let content: &dyn BuiltinContent = &*c;
        let sections = content.sections();
        let DetailSection::TwoColumnLists { right, .. } = &sections[0] else {
            unreachable!("section 0 must be TwoColumnLists");
        };
        assert!(right.items.is_empty());
    }

    #[test]
    fn sections_monospace_name_for_hotkeys() {
        let c = HotkeyClient::new_for_test(None);
        let content: &dyn BuiltinContent = &*c;
        let sections = content.sections();
        let DetailSection::TwoColumnLists { left, .. } = &sections[0] else {
            unreachable!("section 0 must be TwoColumnLists");
        };
        for item in &left.items {
            assert!(item.monospace_name);
        }
    }
}
