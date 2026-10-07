use super::field::{SelectEntries, SubFormField};
use super::*;
use forge_components::{
    BORDER_THIN, FONT_SM, FONT_XS, FONT_XXS, Picker, PickerEvent, PickerItem, PickerLabels, Radius,
    Spacing, body_family, dropdown, mono_family, radius, spacing,
};
use gpui::Rgba;

pub(super) struct SelectPickerForm {
    pub(super) key: String,
    pub(super) picker: Entity<Picker>,
    _sub: Subscription,
}

impl EditSubActionForm {
    pub(super) fn open_select_picker(
        &mut self,
        key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let already_open = self
            .select_picker
            .as_ref()
            .is_some_and(|picker_form| picker_form.key == key);
        if already_open {
            self.close_select_picker(cx);
            return;
        }
        let Some(SelectEntries {
            options,
            selected,
            accepts_typed_value,
        }) = self
            .fields
            .iter()
            .find_map(|field| field.select_entries(&key))
        else {
            return;
        };
        let palette = cx.palette();
        let picker_labels = PickerLabels {
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("actions_sub_select_empty").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let items = select_picker_items(&options);
        let current = Some(SharedString::from(selected));
        let picker = cx.new(|cx| {
            let picker = Picker::new(picker_labels, items, palette, cx).with_current(current);
            if accepts_typed_value {
                picker.with_custom_entry(tr!("config_form_choice_custom").into())
            } else {
                picker
            }
        });
        let sub = cx.subscribe(&picker, Self::on_select_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.select_picker = Some(SelectPickerForm {
            key,
            picker,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_select_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(id) => self.pick_select_option(id.to_string(), cx),
            PickerEvent::Cancelled => self.close_select_picker(cx),
        }
    }

    fn close_select_picker(&mut self, cx: &mut Context<Self>) {
        self.select_picker = None;
        cx.notify();
    }

    pub(super) fn pick_select_option(&mut self, value: String, cx: &mut Context<Self>) {
        let picked = self.select_picker.as_ref().map(|p| p.key.clone());
        if let Some(key) = &picked {
            for field in &mut self.fields {
                match field {
                    SubFormField::Select {
                        key: k, selected, ..
                    } if k == key => *selected = value.clone(),
                    SubFormField::UnitAmount {
                        unit_picker_key,
                        scale,
                        unit,
                        input,
                        ..
                    } if unit_picker_key == key => {
                        let typed = input.read(cx).content().to_owned();
                        if let Some(converted) =
                            scale.amount_after_unit_switch(&typed, unit, &value)
                        {
                            input.update(cx, |input, cx| input.set_content(converted, cx));
                        }
                        *unit = value.clone();
                        field.flag_unit_amount_bound(cx);
                    }
                    _ => {}
                }
            }
        }
        self.select_picker = None;
        let selector_changed = self
            .refinement
            .zip(picked.as_ref())
            .is_some_and(|(refinement, key)| refinement.selector_key == key);
        if selector_changed {
            self.rebuild_refined(cx);
        }
        self.resolve_dependent_selects(cx);
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_select_field(
        &self,
        key: &str,
        label: &str,
        options: &[(String, String)],
        selected: &str,
        open_picker: Option<&SelectPickerForm>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label.to_owned()),
            )
            .child(self.render_select_trigger(key, options, selected, open_picker, palette, cx))
            .into_any_element()
    }

    pub(super) fn render_select_trigger(
        &self,
        key: &str,
        options: &[(String, String)],
        selected: &str,
        open_picker: Option<&SelectPickerForm>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = options
            .iter()
            .find(|(value, _)| value == selected)
            .map(|(_, label)| label.clone());
        let (display, display_color): (String, Rgba) = match selected_label {
            Some(label) => (label, palette.text_primary),
            None if !selected.is_empty() => (selected.to_owned(), palette.text_primary),
            None => (tr!("actions_sub_select_placeholder"), palette.text_faint),
        };

        let key_open = key.to_owned();
        let border_color = if open_picker.is_some() {
            palette.brand
        } else {
            palette.border_input
        };
        let hover_border = palette.brand;
        let trigger = div()
            .id(SharedString::from(format!("actions-sub-select-{key}")))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(px(6.0))
            .px(px(10.0))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.shell)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_select_picker(key_open.clone(), window, cx)
            }))
            .child(
                div()
                    .flex_1()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(display_color)
                    .child(display),
            )
            .child(icon(Icon::ChevronDown, FONT_SM, palette.text_faint));

        let popover = open_picker.map(|form| {
            let view = cx.entity();
            dropdown(form.picker.clone()).on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_select_picker(cx));
            })
        });

        div()
            .relative()
            .w_full()
            .child(trigger)
            .children(popover)
            .into_any_element()
    }

    pub(super) fn open_picker_for(&self, key: &str) -> Option<&SelectPickerForm> {
        self.select_picker
            .as_ref()
            .filter(|picker_form| picker_form.key == key)
    }
}

pub(super) fn select_picker_items(options: &[(String, String)]) -> Vec<PickerItem> {
    options
        .iter()
        .map(|(value, label)| PickerItem {
            id: SharedString::from(value.clone()),
            label: SharedString::from(label.clone()),
            sublabel: None,
            icon: None,
        })
        .collect()
}
