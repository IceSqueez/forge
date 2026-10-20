use super::*;
use forge_components::{
    DateTimePicker, DateTimePickerEvent, DateTimePickerLabels, anchored_popover,
};

pub(super) struct DateTimePickerForm {
    picker: Entity<DateTimePicker>,
    target_input: Entity<TextInput>,
    pos: Point<Pixels>,
    _sub: Subscription,
}

impl EditSubActionForm {
    pub(super) fn open_datetime_picker(
        &mut self,
        target_input: Entity<TextInput>,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_variable_picker(cx);
        let palette = cx.palette();
        let initial = target_input.read(cx).content().to_owned();
        let labels = DateTimePickerLabels {
            now: tr!("actions_sub_datetime_now").into(),
            set: tr!("actions_sub_datetime_set").into(),
            cancel: tr!("common_cancel").into(),
        };
        let picker = cx.new(|cx| DateTimePicker::new(Some(initial.as_str()), labels, palette, cx));
        let sub = cx.subscribe(&picker, Self::on_datetime_event);
        self.datetime_picker = Some(DateTimePickerForm {
            picker,
            target_input,
            pos,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_datetime_event(
        &mut self,
        _picker: Entity<DateTimePicker>,
        event: &DateTimePickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DateTimePickerEvent::Picked(value) => {
                if let Some(form) = self.datetime_picker.take() {
                    let value = value.to_string();
                    form.target_input.update(cx, |input, cx| {
                        input.set_content(value, cx);
                        cx.notify();
                    });
                }
                cx.notify();
            }
            DateTimePickerEvent::Dismissed => self.close_datetime_picker(cx),
        }
    }

    fn close_datetime_picker(&mut self, cx: &mut Context<Self>) {
        self.datetime_picker = None;
        cx.notify();
    }

    pub(super) fn render_datetime_popover(
        &self,
        form: &DateTimePickerForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        anchored_popover(form.pos, form.picker.clone())
            .dismiss_on_escape(&self.datetime_focus)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_datetime_picker(cx));
            })
            .into_any_element()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use gpui::TestAppContext;

    use super::super::variable_insert::tests::{
        WHEN, content_of, open_calendar, open_form_in_scope, stored, when_spec,
    };
    use super::*;

    #[gpui::test]
    fn a_calendar_pick_replaces_a_variable_token_with_the_picked_instant(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open_form_in_scope(
            cx,
            vec![when_spec()],
            stored(&[(WHEN, "%followed_at%")]),
            Vec::new(),
        );
        open_calendar(&form, vcx);
        let picker = vcx.update(|_window, cx| {
            form.read(cx)
                .datetime_picker
                .as_ref()
                .map(|open| open.picker.clone())
                .unwrap()
        });

        vcx.update(|_window, cx| {
            picker.update(cx, |_, cx| {
                cx.emit(DateTimePickerEvent::Picked("2026-10-04T12:00:00Z".into()))
            })
        });
        vcx.run_until_parked();

        assert_eq!(content_of(&form, vcx, WHEN), "2026-10-04T12:00:00Z");
    }
}
