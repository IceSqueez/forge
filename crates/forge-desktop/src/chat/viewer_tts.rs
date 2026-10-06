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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use forge_components::{Platform, ToastKind, tr};
    use forge_storage::voice_aliases::MockVoiceAliasRepo;
    use forge_storage::{AliasId, StorageError, VoiceAlias};
    use forge_voice::{AliasState, EngineId, VoiceId};
    use gpui::{Entity, TestAppContext, VisualTestContext};

    use crate::chat::ChatView;
    use crate::chat::tests::mount_in_window;
    use crate::chat::viewer_actions::ViewerTarget;
    use crate::chat_author::AuthorKey;
    use crate::test_support::{alias_shape, runtime};
    use crate::toasts::Toasts;

    const VIEWER_KEY: &str = "twitch:141981764";
    const SETTLE_ROUNDS: usize = 2000;

    fn id_keyed_viewer() -> ViewerTarget {
        ViewerTarget::new(
            &AuthorKey::by_viewer_id(Platform::Twitch, "141981764"),
            "alice",
        )
    }

    fn stored_alias() -> VoiceAlias {
        VoiceAlias {
            id: AliasId("a1".to_owned()),
            viewer_id: VIEWER_KEY.to_owned(),
            viewer_name: "alice".to_owned(),
            engine_id: EngineId("piper".to_owned()),
            voice_id: VoiceId("amy".to_owned()),
            pitch_semitones: Some(1.5),
            rate_multiplier: None,
            state: AliasState::Active,
        }
    }

    fn finding(found: Option<VoiceAlias>) -> MockVoiceAliasRepo {
        let mut repo = MockVoiceAliasRepo::new();
        repo.expect_find_by_viewer()
            .withf(|key| key == VIEWER_KEY)
            .returning(move |_| Ok(found.clone()));
        repo
    }

    fn settle(
        vcx: &mut VisualTestContext,
        rt: &tokio::runtime::Runtime,
        done: impl Fn(&mut VisualTestContext) -> bool,
    ) {
        for _ in 0..SETTLE_ROUNDS {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            vcx.run_until_parked();
            if done(vcx) {
                return;
            }
        }
        panic!("the drawer never settled");
    }

    fn form_open(vcx: &mut VisualTestContext, view: &Entity<ChatView>) -> bool {
        vcx.update(|_window, cx| view.read(cx).tts_voice.is_some())
    }

    fn toasts(vcx: &mut VisualTestContext) -> Vec<(ToastKind, String)> {
        vcx.update(|_window, cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| (toast.kind, toast.message.to_string()))
                .collect()
        })
    }

    fn open_voice_form(
        vcx: &mut VisualTestContext,
        rt: &tokio::runtime::Runtime,
        view: &Entity<ChatView>,
    ) {
        vcx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_tts_voice(id_keyed_viewer(), window, cx)
            })
        });
        settle(vcx, rt, |vcx| form_open(vcx, view));
    }

    fn press_enter(vcx: &mut VisualTestContext) {
        vcx.simulate_keystrokes("enter");
    }

    #[gpui::test]
    fn set_tts_voice_saves_the_stored_alias_back_under_the_viewer_key(cx: &mut TestAppContext) {
        let rt = runtime();
        let saved = Arc::new(Mutex::new(Vec::new()));
        let mut repo = finding(Some(stored_alias()));
        let sink = Arc::clone(&saved);
        repo.expect_upsert().returning(move |alias| {
            sink.lock().expect("saved").push(alias.clone());
            Ok(alias.clone())
        });
        let (view, vcx) = mount_in_window(cx, &rt, repo);

        open_voice_form(vcx, &rt, &view);
        press_enter(vcx);
        settle(vcx, &rt, |_| !saved.lock().expect("saved").is_empty());

        let saved = saved
            .lock()
            .expect("saved")
            .iter()
            .map(alias_shape)
            .collect::<Vec<_>>();
        assert_eq!(saved, [alias_shape(&stored_alias())]);
    }

    #[gpui::test]
    fn a_saved_voice_closes_the_form_with_a_success_toast(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut repo = finding(None);
        repo.expect_upsert().returning(|alias| Ok(alias.clone()));
        let (view, vcx) = mount_in_window(cx, &rt, repo);

        open_voice_form(vcx, &rt, &view);
        press_enter(vcx);
        settle(vcx, &rt, |vcx| !form_open(vcx, &view));

        assert_eq!(
            toasts(vcx),
            [(ToastKind::Success, tr!("chat_drawer_tts_voice_saved"))]
        );
    }

    #[gpui::test]
    fn a_voice_another_alias_holds_shows_the_taken_error_and_keeps_the_form_usable(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let attempts = Arc::new(AtomicUsize::new(0));
        let mut repo = finding(None);
        let counter = Arc::clone(&attempts);
        repo.expect_upsert().returning(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(StorageError::AliasViewerTaken)
        });
        let (view, vcx) = mount_in_window(cx, &rt, repo);

        open_voice_form(vcx, &rt, &view);
        for _ in 0..2 {
            let before = attempts.load(Ordering::SeqCst);
            press_enter(vcx);
            settle(vcx, &rt, |vcx| {
                attempts.load(Ordering::SeqCst) > before && toasts(vcx).len() > before
            });
        }

        let taken = (ToastKind::Error, tr!("tts_aliases_viewer_taken"));
        assert_eq!(
            (form_open(vcx, &view), toasts(vcx)),
            (true, vec![taken.clone(), taken])
        );
    }

    #[gpui::test]
    fn a_failed_alias_lookup_reports_an_error_instead_of_opening_the_form(cx: &mut TestAppContext) {
        let rt = runtime();
        let mut repo = MockVoiceAliasRepo::new();
        repo.expect_find_by_viewer().returning(|_| {
            Err(StorageError::Connection {
                reason: "closed".to_owned(),
            })
        });
        let (view, vcx) = mount_in_window(cx, &rt, repo);

        vcx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_tts_voice(id_keyed_viewer(), window, cx)
            })
        });
        settle(vcx, &rt, |vcx| !toasts(vcx).is_empty());

        assert_eq!(
            (
                form_open(vcx, &view),
                toasts(vcx)
                    .into_iter()
                    .map(|(kind, _)| kind)
                    .collect::<Vec<_>>()
            ),
            (false, vec![ToastKind::Error])
        );
    }

    #[gpui::test]
    fn blocking_from_the_drawer_toasts_whether_the_block_was_stored(cx: &mut TestAppContext) {
        let failure = || StorageError::Connection {
            reason: "closed".to_owned(),
        };
        for (lookup_fails, expected) in [(false, ToastKind::Success), (true, ToastKind::Error)] {
            let rt = runtime();
            let mut repo = MockVoiceAliasRepo::new();
            repo.expect_find_by_viewer()
                .withf(|key| key == VIEWER_KEY)
                .returning(move |_| {
                    if lookup_fails {
                        Err(failure())
                    } else {
                        Ok(None)
                    }
                });
            repo.expect_upsert()
                .withf(|alias| alias.viewer_id == VIEWER_KEY && alias.state == AliasState::Blocked)
                .returning(|alias| Ok(alias.clone()));
            let (view, vcx) = mount_in_window(cx, &rt, repo);

            vcx.update(|_window, cx| {
                view.update(cx, |view, cx| view.block_tts_viewer(id_keyed_viewer(), cx))
            });
            settle(vcx, &rt, |vcx| !toasts(vcx).is_empty());

            assert_eq!(
                toasts(vcx)
                    .into_iter()
                    .map(|(kind, _)| kind)
                    .collect::<Vec<_>>(),
                [expected],
                "lookup fails = {lookup_fails}"
            );
        }
    }

    #[gpui::test]
    fn a_viewer_known_only_by_name_can_neither_get_a_voice_nor_be_blocked(cx: &mut TestAppContext) {
        let rt = runtime();
        let repo_calls = Arc::new(AtomicUsize::new(0));
        let mut repo = MockVoiceAliasRepo::new();
        let finds = Arc::clone(&repo_calls);
        repo.expect_find_by_viewer().returning(move |_| {
            finds.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        });
        let upserts = Arc::clone(&repo_calls);
        repo.expect_upsert().returning(move |alias| {
            upserts.fetch_add(1, Ordering::SeqCst);
            Ok(alias.clone())
        });
        let (view, vcx) = mount_in_window(cx, &rt, repo);
        let name_only = ViewerTarget::new(&AuthorKey::by_name(Platform::Twitch, "alice"), "alice");

        vcx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_tts_voice(name_only.clone(), window, cx);
                view.block_tts_viewer(name_only, cx);
            })
        });
        for _ in 0..8 {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            vcx.run_until_parked();
        }

        assert_eq!(
            (
                repo_calls.load(Ordering::SeqCst),
                form_open(vcx, &view),
                toasts(vcx)
            ),
            (0, false, Vec::new())
        );
    }
}
