use std::sync::Arc;

use forge_components::{ToastKind, tr};
use forge_storage::{StorageError, VoiceAlias};
use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::ChatView;
use super::viewer_actions::ViewerTarget;
use crate::async_bridge;
use crate::toasts::PushToast;
use crate::voice_alias_form::{
    AliasForm, AliasFormEvent, AliasIdentity, AliasValues, platform_scope,
};
use crate::voice_alias_store::{block_viewer, save_alias};

pub(super) struct TtsVoiceHost {
    pub(super) view: Entity<AliasForm>,
    _events: Subscription,
}

impl ChatView {
    pub(super) fn block_tts_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        cx.notify();
        let Some(key) = target.tts_alias_key() else {
            return;
        };
        let repo = Arc::clone(&self.voice_alias_repo);
        let speak = self.speak.clone();
        let name = target.name;
        async_bridge::run_async(
            &self.rt_handle,
            async move { block_viewer(repo.as_ref(), speak.as_ref(), key, name).await },
            |_this, outcome, cx| match outcome {
                Ok(_) => cx.push_toast(ToastKind::Success, tr!("chat_drawer_block_tts_sent")),
                Err(e) => cx.push_toast(
                    ToastKind::Error,
                    tr!("chat_drawer_block_tts_failed", error = e.to_string()),
                ),
            },
            cx,
        );
    }

    pub(super) fn open_tts_voice(
        &mut self,
        target: ViewerTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drawer_menu_open = None;
        cx.notify();
        if self.tts_voice.is_some() {
            return;
        }
        let Some(key) = target.tts_alias_key() else {
            return;
        };
        let repo = Arc::clone(&self.voice_alias_repo);
        let lookup_key = key.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(repo.find_by_viewer(&lookup_key).await);
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(found) = rx.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| match found {
                Ok(existing) => this.show_tts_voice_form(target, key, existing, window, cx),
                Err(e) => cx.push_toast(
                    ToastKind::Error,
                    tr!("chat_drawer_tts_voice_failed", error = e.to_string()),
                ),
            });
        })
        .detach();
    }

    fn show_tts_voice_form(
        &mut self,
        target: ViewerTarget,
        key: String,
        existing: Option<VoiceAlias>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tts_voice.is_some() {
            return;
        }
        let identity = AliasIdentity {
            key,
            name: target.name,
            platform: platform_scope(target.platform),
        };
        let editing = existing.as_ref().map(|alias| alias.id.clone());
        let values = existing.as_ref().map(AliasValues::of).unwrap_or_default();
        let view = cx.new(|cx| AliasForm::new(editing, Some(identity), true, values, cx));
        view.update(cx, |form, cx| form.focus(window, cx));
        let events = cx.subscribe(&view, Self::on_tts_voice_event);
        self.tts_voice = Some(TtsVoiceHost {
            view,
            _events: events,
        });
        cx.notify();
    }

    fn on_tts_voice_event(
        &mut self,
        form: Entity<AliasForm>,
        event: &AliasFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            AliasFormEvent::Submit(alias) => self.save_tts_voice(form, alias.clone(), cx),
            AliasFormEvent::Cancel => self.close_tts_voice(cx),
        }
    }

    fn save_tts_voice(
        &mut self,
        form: Entity<AliasForm>,
        alias: VoiceAlias,
        cx: &mut Context<Self>,
    ) {
        let replaces_existing = form.update(cx, |form, cx| {
            form.set_saving(true, cx);
            form.is_editing()
        });
        let repo = Arc::clone(&self.voice_alias_repo);
        let speak = self.speak.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { save_alias(repo.as_ref(), speak.as_ref(), &alias, replaces_existing).await },
            move |this, outcome, cx| match outcome {
                Ok(_) => {
                    this.close_tts_voice(cx);
                    cx.push_toast(ToastKind::Success, tr!("chat_drawer_tts_voice_saved"));
                }
                Err(e) => {
                    form.update(cx, |form, cx| form.set_saving(false, cx));
                    let message = match e {
                        StorageError::AliasViewerTaken => tr!("tts_aliases_viewer_taken"),
                        other => tr!("chat_drawer_tts_voice_failed", error = other.to_string()),
                    };
                    cx.push_toast(ToastKind::Error, message);
                }
            },
            cx,
        );
    }

    fn close_tts_voice(&mut self, cx: &mut Context<Self>) {
        self.tts_voice = None;
        cx.notify();
    }
}
