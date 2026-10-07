use forge_components::Icon;
use forge_storage::OverlayId;
use gpui::{Context, Entity, Subscription, prelude::*};

use super::OverlaysView;
use super::form_modal::{OverlayFormEvent, OverlayFormLaunch, OverlayFormModal, OverlayTypeChoice};

pub(super) struct OpenForm {
    pub(super) view: Entity<OverlayFormModal>,
    _sub: Subscription,
}

impl OverlaysView {
    fn type_choices(&self) -> Vec<OverlayTypeChoice> {
        let mut choices: Vec<OverlayTypeChoice> = self
            .handles
            .kinds
            .all()
            .map(|descriptor| OverlayTypeChoice {
                kind_id: descriptor.id().to_owned(),
                label: descriptor.label().to_owned(),
                summary: descriptor.summary().to_owned(),
                icon: Icon::from_name(descriptor.icon_name()),
            })
            .collect();
        choices.sort_by(|a, b| a.label.cmp(&b.label));
        choices
    }

    pub(super) fn open_create_form(&mut self, cx: &mut Context<Self>) {
        let types = self.type_choices();
        let kind_id = types
            .first()
            .map(|choice| choice.kind_id.clone())
            .unwrap_or_default();
        self.open_form(
            OverlayFormLaunch {
                target: None,
                display_name: String::new(),
                kind_id,
                types,
            },
            cx,
        );
    }

    pub(super) fn open_rename_form(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        let Some(definition) = self
            .index_of(&id)
            .map(|index| &self.registry.overlays[index])
        else {
            return;
        };
        let launch = OverlayFormLaunch {
            display_name: definition.display_name.clone(),
            kind_id: definition.kind_id.clone(),
            target: Some(id),
            types: self.type_choices(),
        };
        self.open_form(launch, cx);
    }

    fn open_form(&mut self, launch: OverlayFormLaunch, cx: &mut Context<Self>) {
        let view = cx.new(|cx| OverlayFormModal::new(launch, cx));
        let sub = cx.subscribe(&view, Self::on_form_event);
        self.form = Some(OpenForm { view, _sub: sub });
        self.registry.menu_open = None;
        cx.notify();
    }

    pub(super) fn close_form(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        cx.notify();
    }

    fn on_form_event(
        &mut self,
        _view: Entity<OverlayFormModal>,
        event: &OverlayFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            OverlayFormEvent::Submit {
                target,
                display_name,
                kind_id,
            } => match target {
                Some(id) => self.rename(id.clone(), display_name.clone(), cx),
                None => self.create(display_name.clone(), kind_id.clone(), cx),
            },
            OverlayFormEvent::Cancel => self.close_form(cx),
        }
    }
}
