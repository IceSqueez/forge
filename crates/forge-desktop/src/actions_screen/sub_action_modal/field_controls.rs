use super::field::SubFormField;
use super::*;
use crate::async_bridge;
use forge_components::{
    FONT_XS, FONT_XXS, Spacing, body_family, field_label, ghost_button_with_icon, mono_family,
    spacing, toggle,
};

impl EditSubActionForm {
    fn toggle_sub_field(&mut self, key: String, cx: &mut Context<Self>) {
        for field in &mut self.fields {
            if let SubFormField::Bool { key: k, value, .. } = field
                && *k == key
            {
                *value = !*value;
            }
        }
        cx.notify();
    }

    fn browse_sub_field(&mut self, input: Entity<TextInput>, cx: &mut Context<Self>) {
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_file(None),
            move |_this, result, cx| {
                if let Ok(path) = result {
                    input.update(cx, |input, cx| {
                        input.set_content(path.to_string_lossy().into_owned(), cx);
                        cx.notify();
                    });
                }
            },
            cx,
        );
    }

    pub(super) fn input_control(
        &self,
        key: &str,
        browse: bool,
        datetime: bool,
        input: &Entity<TextInput>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if browse {
            let target_input = input.clone();
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .child(div().flex_1().child(input.clone()))
                .child(
                    ghost_button_with_icon(Icon::Folder, tr!("actions_sub_file_browse"), palette)
                        .on_click(
                            SharedString::from(format!("actions-sub-browse-{key}")),
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.browse_sub_field(target_input.clone(), cx)
                            }),
                        ),
                )
                .into_any_element()
        } else if datetime {
            let target_input = input.clone();
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .child(div().flex_1().child(input.clone()))
                .child(
                    ghost_button_with_icon(
                        Icon::Calendar,
                        tr!("actions_sub_datetime_pick"),
                        palette,
                    )
                    .on_click(
                        SharedString::from(format!("actions-sub-datetime-{key}")),
                        cx.listener(move |this, ev: &ClickEvent, _, cx| {
                            this.open_datetime_picker(target_input.clone(), ev.position(), cx)
                        }),
                    ),
                )
                .into_any_element()
        } else {
            input.clone().into_any_element()
        }
    }
}

pub(super) fn bool_row(
    key: &str,
    label: &str,
    value: bool,
    palette: &ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> AnyElement {
    let toggle_key = key.to_owned();
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Sm, Density::Cozy))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(label.to_owned()),
        )
        .child(toggle(value, palette).on_click(
            SharedString::from(format!("actions-sub-toggle-{key}")),
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.toggle_sub_field(toggle_key.clone(), cx)
            }),
        ))
        .into_any_element()
}

pub(super) fn hint_block(label: &str, palette: &ForgePalette) -> AnyElement {
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
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(tr!("action_editor_branch_modal_hint")),
        )
        .into_any_element()
}

pub(super) fn field_wrap(label: &str, control: AnyElement, palette: &ForgePalette) -> AnyElement {
    field_label(palette, label.to_owned(), control)
        .tone(palette.text_muted)
        .into_any_element()
}

pub(super) fn multiline_field(
    label: &str,
    tag: Option<&'static str>,
    control: AnyElement,
    palette: &ForgePalette,
) -> AnyElement {
    let caption = |text: SharedString| {
        div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(text)
    };
    let header = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .child(caption(label.to_owned().into()))
        .children(tag.map(|tag| caption(tag.into())));
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .child(header)
        .child(control)
        .into_any_element()
}
