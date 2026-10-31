use super::console::LogTag;
use super::input_parsing::{
    ARRAY_INPUT_EXAMPLE, DATETIME_INPUT_EXAMPLE, OBJECT_INPUT_EXAMPLE, ScriptInputReason,
    ScriptInputRejection, input_syntax_example, parse_input_to_variant,
};
use super::*;
use crate::navigation::Screen;
use crate::repo_failure::RepoFailure;
use crate::script_lifecycle::delete_hint;
use forge_components::ToastKind;
use forge_storage::ScriptRepo;
use forge_types::{Variant, VariantKind};
use std::collections::BTreeMap;
use time::OffsetDateTime;

const ALL_KINDS: [VariantKind; 7] = [
    VariantKind::Int,
    VariantKind::Float,
    VariantKind::Bool,
    VariantKind::String,
    VariantKind::Datetime,
    VariantKind::Array,
    VariantKind::Object,
];

fn parse(kind: VariantKind, raw: &str) -> Result<Variant, String> {
    crate::i18n::install_language(forge_storage::Language::En);
    parse_input_to_variant("amount", kind, raw).map_err(|e| e.user_message())
}

#[test]
fn every_declared_kind_accepts_a_well_formed_value() {
    let mut object = BTreeMap::new();
    object.insert("key".to_owned(), Variant::String("value".into()));

    for (kind, raw, expected) in [
        (VariantKind::Int, " 42 ", Variant::Int(42)),
        (VariantKind::Int, "-7", Variant::Int(-7)),
        (VariantKind::Float, " 1.5", Variant::float(1.5).unwrap()),
        (VariantKind::Bool, "true", Variant::Bool(true)),
        (VariantKind::Bool, " false ", Variant::Bool(false)),
        (
            VariantKind::Array,
            " [12, 13, 14] ",
            Variant::Array(vec![Variant::Int(12), Variant::Int(13), Variant::Int(14)]),
        ),
        (VariantKind::Array, "[]", Variant::Array(vec![])),
        (
            VariantKind::Object,
            r#"{"key": "value"}"#,
            Variant::Object(object.clone()),
        ),
        (VariantKind::Object, "{}", Variant::Object(BTreeMap::new())),
        (
            VariantKind::Datetime,
            " 1970-01-01T00:00:00Z ",
            Variant::Datetime(OffsetDateTime::UNIX_EPOCH),
        ),
    ] {
        assert_eq!(
            parse(kind, raw).unwrap(),
            expected,
            "{kind:?} input {raw:?}"
        );
    }
}

#[test]
fn a_string_input_is_taken_verbatim_while_every_other_kind_is_trimmed() {
    assert_eq!(
        parse(VariantKind::String, "  padded  ").unwrap(),
        Variant::String("  padded  ".into()),
        "a script may well want the whitespace it was handed"
    );
}

#[test]
fn every_declared_kind_names_the_field_when_it_rejects_input() {
    for (kind, raw) in [
        (VariantKind::Int, ""),
        (VariantKind::Int, "1.5"),
        (VariantKind::Int, "9223372036854775808"),
        (VariantKind::Float, "1,5"),
        (VariantKind::Float, ""),
        (VariantKind::Bool, "TRUE"),
        (VariantKind::Bool, "1"),
        (VariantKind::Array, "[1,"),
        (VariantKind::Array, "12"),
        (VariantKind::Array, "[1, 2] trailing"),
        (VariantKind::Object, "not json"),
        (VariantKind::Object, "[1, 2]"),
        (VariantKind::Datetime, "2026-01-31T18:04:00"),
    ] {
        let err = parse(kind, raw).unwrap_err();
        assert!(
            err.contains("amount"),
            "{kind:?} input {raw:?} must tell the user which field failed: {err}"
        );
    }
}

#[test]
fn a_float_input_that_parses_but_is_not_finite_is_rejected() {
    for raw in ["NaN", "inf", "-inf"] {
        let err = parse(VariantKind::Float, raw).unwrap_err();
        assert!(
            err.contains("invalid float"),
            "input {raw:?} must not reach a script as a non-finite float: {err}"
        );
    }
}

#[test]
fn well_formed_json_of_the_wrong_kind_is_rejected_with_the_example_for_the_declared_kind() {
    for (kind, raw, shape, example) in [
        (
            VariantKind::Array,
            r#"{"key": "value"}"#,
            "a JSON array",
            ARRAY_INPUT_EXAMPLE,
        ),
        (
            VariantKind::Object,
            "[12, 13, 14]",
            "a JSON object",
            OBJECT_INPUT_EXAMPLE,
        ),
    ] {
        let err = parse(kind, raw).unwrap_err();
        assert!(err.contains(shape), "{kind:?} input {raw}: {err}");
        assert!(
            err.contains(example),
            "{kind:?} input {raw} must show the syntax it wants: {err}"
        );
    }
}

#[test]
fn a_variant_rejection_inside_a_json_input_is_reported_instead_of_the_syntax_example() {
    let err = parse(VariantKind::Array, "[1, null]").unwrap_err();

    assert!(
        err.contains("null is not a supported Variant value"),
        "the value, not the syntax, is what the user has to fix: {err}"
    );
    assert!(
        !err.contains(ARRAY_INPUT_EXAMPLE),
        "a well-formed array is not a syntax problem: {err}"
    );
}

#[test]
fn a_tagged_looking_object_input_stays_an_object_rather_than_decoding_to_its_tag() {
    let parsed = parse(VariantKind::Object, r#"{"type": "int", "value": 7}"#).unwrap();

    assert_eq!(
        parsed.as_object().unwrap().len(),
        2,
        "a script asking for an object gets the two keys the user typed: {parsed:?}"
    );
}

#[test]
fn the_syntax_example_offered_for_a_kind_parses_back_into_that_kind() {
    for kind in ALL_KINDS {
        let Some(example) = input_syntax_example(kind) else {
            continue;
        };
        let parsed = parse(kind, example).unwrap();
        assert_eq!(
            VariantKind::from_variant(&parsed),
            kind,
            "the placeholder teaches {example}, which must be valid input"
        );
    }
}

mod unsaved_work {
    use forge_components::{Density, ThemeId};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_storage::script::MockScriptRepo;
    use forge_storage::{Language, StorageError};
    use forge_types::IntegrationId;
    use gpui::{AnyWindowHandle, AppContext, TestAppContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::navigation_guard::{LeaveTarget, NavigationGuard};
    use crate::screen_shortcuts::ScreenShortcuts;
    use crate::test_support::{
        StubEventLog, pump, pump_until, runtime, test_backend_with_scripts,
        test_backend_with_scripts_and_actions,
    };
    use crate::unsaved_work::UnsavedWorkHandle;
    use forge_components::Presentation;

    const SAVED_BODY: &str = "let greeting = \"hi\";";
    const EDITED_BODY: &str = "let greeting = \"hello\";";
    const BROKEN_BODY: &str = "let greeting = ;";

    struct NothingDisabled;

    impl IntegrationAvailability for NothingDisabled {
        fn is_disabled(&self, _: &IntegrationId) -> bool {
            false
        }
    }

    struct Blank;

    impl Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    enum Store {
        Accepts,
        Fails,
        NeverCalled,
        Counted(Arc<AtomicUsize>),
    }

    fn repo(store: Store) -> MockScriptRepo {
        let mut repo = MockScriptRepo::new();
        repo.expect_list().returning(|| Ok(Vec::new()));
        repo.expect_telemetry().returning(|_| {
            Err(StorageError::NotFound {
                key: "telemetry".to_owned(),
            })
        });
        match store {
            Store::Accepts => {
                repo.expect_save().times(1).returning(|_| Ok(()));
            }
            Store::Fails => {
                repo.expect_save().times(1).returning(|_| {
                    Err(StorageError::Connection {
                        reason: "disk full".to_owned(),
                    })
                });
            }
            Store::NeverCalled => {
                repo.expect_save().never();
            }
            Store::Counted(stores) => {
                repo.expect_save().returning(move |_| {
                    stores.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                });
            }
        }
        repo
    }

    fn record() -> ScriptRecord {
        let now = OffsetDateTime::now_utc();
        ScriptRecord {
            id: ScriptId::new(),
            name: "greet".to_owned(),
            body: SAVED_BODY.to_owned(),
            contract: ScriptContract::default(),
            enabled: true,
            created_at: now,
            last_modified: now,
        }
    }

    #[gpui::test]
    fn a_failed_action_list_reports_a_load_failure_instead_of_unlinked_scripts(
        cx: &mut TestAppContext,
    ) {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
        let rt = runtime();
        let bus = {
            let _entered = rt.enter();
            EventBus::new(Arc::new(StubEventLog))
        };
        let mut actions = forge_storage::action::MockActionRepo::new();
        actions.expect_list().returning(|| {
            Err(StorageError::Connection {
                reason: "disk full".to_owned(),
            })
        });
        let backend = test_backend_with_scripts_and_actions(
            Arc::new(repo(Store::NeverCalled)),
            Arc::new(actions),
        );
        let handle = rt.handle().clone();
        let view = cx.new(|cx| {
            ScriptEditorView::new(
                backend,
                Arc::new(ScriptRegistry::new()),
                bus,
                Arc::new(NothingDisabled),
                handle,
                cx,
            )
        });

        pump_until(&rt, || {
            cx.run_until_parked();
            !view.read_with(cx, |view, _| view.loading)
        });

        let failures = view.read_with(cx, |view, _| {
            view.console
                .iter()
                .filter(|line| matches!(line.tag, LogTag::Err))
                .count()
        });
        assert_eq!(failures, 1);
    }

    struct Rig {
        rt: tokio::runtime::Runtime,
        view: Entity<ScriptEditorView>,
        guard: NavigationGuard,
    }

    impl Rig {
        fn mount(cx: &mut TestAppContext, store: Store) -> Self {
            install_language(Language::En);
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let bus = {
                let _entered = rt.enter();
                EventBus::new(Arc::new(StubEventLog))
            };
            let backend = test_backend_with_scripts(Arc::new(repo(store)));
            let handle = rt.handle().clone();
            let view = cx.new(|cx| {
                ScriptEditorView::new(
                    backend,
                    Arc::new(ScriptRegistry::new()),
                    bus,
                    Arc::new(NothingDisabled),
                    handle,
                    cx,
                )
            });
            view.update(cx, |view, cx| view.apply_script_opened(Ok(record()), cx));
            let window: AnyWindowHandle = cx.add_window(|_, _| Blank).into();
            let mut guard = NavigationGuard::new(window);
            guard.watch(Some(UnsavedWorkHandle::new(view.clone())));
            let rig = Self { rt, view, guard };
            rig.settle(cx);
            rig
        }

        fn settle(&self, cx: &mut TestAppContext) {
            pump(&self.rt);
            pump_until(&self.rt, || {
                cx.run_until_parked();
                !self.saving(cx)
            });
        }

        fn type_body(&self, body: &str, cx: &mut TestAppContext) {
            let body = body.to_owned();
            self.view.update(cx, |view, cx| {
                view.code_input
                    .update(cx, |area, cx| area.set_content(body, cx));
            });
            cx.run_until_parked();
        }

        fn body(&self, cx: &mut TestAppContext) -> String {
            self.view
                .read_with(cx, |view, cx| view.code_input.read(cx).content().to_owned())
        }

        fn unsaved(&self, cx: &mut TestAppContext) -> bool {
            self.view
                .read_with(cx, |view, cx| view.has_unsaved_work(cx))
        }

        fn saving(&self, cx: &mut TestAppContext) -> bool {
            self.view
                .read_with(cx, |view, _| view.is_saving_unsaved_work())
        }

        fn save_and_leave(&mut self, cx: &mut TestAppContext) -> Option<LeaveTarget> {
            self.guard.ask(LeaveTarget::Screen(Screen::Home));
            cx.update(|cx| self.guard.save(cx))
        }
    }

    #[gpui::test]
    fn only_an_edit_of_the_open_script_counts_as_unsaved(cx: &mut TestAppContext) {
        let rig = Rig::mount(cx, Store::NeverCalled);
        let opened = rig.unsaved(cx);

        rig.type_body(EDITED_BODY, cx);
        let edited = rig.unsaved(cx);

        assert_eq!((opened, edited), (false, true));
    }

    #[gpui::test]
    fn discard_restores_the_saved_body(cx: &mut TestAppContext) {
        let mut rig = Rig::mount(cx, Store::NeverCalled);
        rig.type_body(EDITED_BODY, cx);
        rig.guard.ask(LeaveTarget::Screen(Screen::Home));

        let released = cx.update(|cx| rig.guard.discard(cx));
        cx.run_until_parked();

        assert_eq!(
            (released, rig.body(cx), rig.unsaved(cx)),
            (
                Some(LeaveTarget::Screen(Screen::Home)),
                SAVED_BODY.to_owned(),
                false
            )
        );
    }

    #[gpui::test]
    fn a_syntax_error_refuses_save_and_leave_without_storing(cx: &mut TestAppContext) {
        let mut rig = Rig::mount(cx, Store::NeverCalled);
        rig.type_body(BROKEN_BODY, cx);

        let released = rig.save_and_leave(cx);
        rig.settle(cx);

        assert_eq!(
            (released, rig.unsaved(cx), rig.body(cx)),
            (None, true, BROKEN_BODY.to_owned())
        );
    }

    #[gpui::test]
    fn save_and_leave_follows_the_stored_outcome(cx: &mut TestAppContext) {
        for (store, expected) in [
            (Store::Accepts, Some(LeaveTarget::Screen(Screen::Home))),
            (Store::Fails, None),
        ] {
            let mut rig = Rig::mount(cx, store);
            rig.type_body(EDITED_BODY, cx);

            let on_click = rig.save_and_leave(cx);
            let storing = rig.saving(cx);
            let while_storing = cx.update(|cx| rig.guard.settle(cx));
            rig.settle(cx);
            let settled = cx.update(|cx| rig.guard.settle(cx));

            assert_eq!(
                (on_click, storing, while_storing, rig.saving(cx), settled),
                (None, true, None, false, expected.clone()),
                "{expected:?}"
            );
        }
    }

    #[gpui::test]
    fn a_save_asked_for_while_one_is_storing_is_ignored(cx: &mut TestAppContext) {
        let stores = Arc::new(AtomicUsize::new(0));
        let rig = Rig::mount(cx, Store::Counted(Arc::clone(&stores)));
        rig.type_body(EDITED_BODY, cx);

        let asked = [(); 2].map(|()| {
            rig.view
                .update(cx, |view, cx| view.start_saving_unsaved_work(cx))
        });
        rig.settle(cx);

        assert_eq!(
            (asked, stores.load(Ordering::SeqCst), rig.unsaved(cx)),
            ([true, true], 1, false)
        );
    }

    struct ShellHosting {
        editor: Entity<ScriptEditorView>,
        focus: gpui::FocusHandle,
        shell_saves: Arc<AtomicUsize>,
    }

    impl Render for ShellHosting {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let shell_saves = Arc::clone(&self.shell_saves);
            div()
                .key_context(crate::actions::SHELL_CONTEXT)
                .track_focus(&self.focus)
                .on_action(move |_: &crate::actions::SaveFocused, _, _| {
                    shell_saves.fetch_add(1, Ordering::SeqCst);
                })
                .size_full()
                .child(self.editor.clone())
        }
    }

    fn press_save_chord(
        rig: &Rig,
        cx: &mut TestAppContext,
        in_code_editor: bool,
    ) -> Arc<AtomicUsize> {
        cx.update(crate::actions::register_key_bindings);
        let shell_saves = Arc::new(AtomicUsize::new(0));
        let hosted = rig.view.clone();
        let counted = Arc::clone(&shell_saves);
        let window = cx.add_window(|_, cx| ShellHosting {
            editor: hosted,
            focus: cx.focus_handle(),
            shell_saves: counted,
        });
        let vcx = &mut gpui::VisualTestContext::from_window(window.into(), cx);
        vcx.update(|window, cx| {
            if in_code_editor {
                let editor = rig.view.read(cx).code_input.clone();
                editor.update(cx, |editor, cx| editor.focus(window, cx));
            } else {
                let focus = window
                    .root::<ShellHosting>()
                    .flatten()
                    .map(|host| host.read(cx).focus.clone());
                if let Some(focus) = focus {
                    window.focus(&focus, cx);
                }
            }
        });
        vcx.run_until_parked();
        vcx.simulate_keystrokes("secondary-s");
        rig.settle(cx);
        shell_saves
    }

    #[gpui::test]
    fn the_save_chord_in_the_code_editor_never_reaches_the_shell_save(cx: &mut TestAppContext) {
        let stores = Arc::new(AtomicUsize::new(0));
        let rig = Rig::mount(cx, Store::Counted(Arc::clone(&stores)));
        rig.type_body(EDITED_BODY, cx);

        let shell_saves = press_save_chord(&rig, cx, true);

        assert_eq!(
            (
                shell_saves.load(Ordering::SeqCst),
                stores.load(Ordering::SeqCst)
            ),
            (0, 1)
        );
    }

    #[gpui::test]
    fn the_save_chord_outside_the_code_editor_goes_to_the_shell_save(cx: &mut TestAppContext) {
        let rig = Rig::mount(cx, Store::NeverCalled);

        let shell_saves = press_save_chord(&rig, cx, false);

        assert_eq!(shell_saves.load(Ordering::SeqCst), 1);
    }

    #[gpui::test]
    fn the_save_shortcut_stores_the_edited_script_unless_a_prompt_is_showing(
        cx: &mut TestAppContext,
    ) {
        for (delete_prompt, expected, case) in
            [(false, 1, "no prompt"), (true, 0, "delete prompt showing")]
        {
            let stores = Arc::new(AtomicUsize::new(0));
            let rig = Rig::mount(cx, Store::Counted(Arc::clone(&stores)));
            rig.type_body(EDITED_BODY, cx);
            if delete_prompt {
                rig.view
                    .update(cx, |view, _| view.pending_delete.request(ScriptId::new()));
            }

            rig.view.update(cx, |view, cx| view.save_shortcut(cx));
            rig.settle(cx);

            assert_eq!(stores.load(Ordering::SeqCst), expected, "{case}");
        }
    }

    #[gpui::test]
    fn the_run_shortcut_runs_the_open_script_unless_a_prompt_is_showing(cx: &mut TestAppContext) {
        for (delete_prompt, expected, case) in
            [(false, 1, "no prompt"), (true, 0, "delete prompt showing")]
        {
            let rig = Rig::mount(cx, Store::NeverCalled);
            if delete_prompt {
                rig.view
                    .update(cx, |view, _| view.pending_delete.request(ScriptId::new()));
            }

            rig.view.update(cx, |view, cx| view.run_shortcut(cx));

            let runs = rig.view.read_with(cx, |view, _| {
                view.console
                    .iter()
                    .filter(|line| matches!(line.tag, LogTag::Run))
                    .count()
            });
            assert_eq!(runs, expected, "{case}");
        }
    }

    fn started_runs(rig: &Rig, cx: &mut TestAppContext) -> usize {
        rig.view.read_with(cx, |view, _| {
            view.console
                .iter()
                .filter(|line| matches!(line.tag, LogTag::Run))
                .count()
        })
    }

    #[gpui::test]
    fn a_run_asked_for_while_one_is_in_flight_is_ignored_until_it_finishes(
        cx: &mut TestAppContext,
    ) {
        let rig = Rig::mount(cx, Store::NeverCalled);

        rig.view.update(cx, |view, cx| {
            view.run_shortcut(cx);
            view.run_shortcut(cx);
        });
        let while_running = started_runs(&rig, cx);

        rig.view.update(cx, |view, cx| {
            view.apply_run_finished(Err(RepoFailure::operation("stopped")), cx);
            view.run_shortcut(cx);
        });

        assert_eq!((while_running, started_runs(&rig, cx)), (1, 2));
    }
}

mod lifecycle {
    use forge_components::{Density, Presentation, ThemeId, Toasts};
    use forge_storage::{Language, ScriptReferrer};
    use forge_types::IntegrationId;
    use gpui::{AnyWindowHandle, AppContext, TestAppContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        MemoryScripts, StubEventLog, pump, runtime, test_backend_with_scripts,
    };

    const OLD_NAME: &str = "greet";
    const NEW_NAME: &str = "welcome";
    const BODY: &str = "let greeting = 1;";

    struct NothingDisabled;

    impl IntegrationAvailability for NothingDisabled {
        fn is_disabled(&self, _: &IntegrationId) -> bool {
            false
        }
    }

    struct Blank;

    impl Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn referrer(name: &str) -> ScriptReferrer {
        ScriptReferrer {
            action_id: ActionId::new(),
            action_name: name.to_owned(),
        }
    }

    fn references(literal: &[&str], templated: &[&str]) -> ScriptReferences {
        ScriptReferences {
            literal: literal.iter().map(|n| referrer(n)).collect(),
            templated: templated.iter().map(|n| referrer(n)).collect(),
        }
    }

    struct Rig {
        rt: tokio::runtime::Runtime,
        view: Entity<ScriptEditorView>,
        scripts: Arc<MemoryScripts>,
        registry: Arc<ScriptRegistry>,
        id: ScriptId,
    }

    impl Rig {
        fn mount(cx: &mut TestAppContext, uses: ScriptReferences) -> Self {
            install_language(Language::En);
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
                cx.set_global(Toasts::new());
            });
            let rt = runtime();
            let now = OffsetDateTime::now_utc();
            let id = ScriptId::new();
            let scripts = Arc::new(
                MemoryScripts::holding([ScriptRecord {
                    id,
                    name: OLD_NAME.to_owned(),
                    body: BODY.to_owned(),
                    contract: ScriptContract::default(),
                    enabled: true,
                    created_at: now,
                    last_modified: now,
                }])
                .referenced_by(OLD_NAME, uses),
            );
            let registry = Arc::new(ScriptRegistry::new());
            rt.block_on(registry.load_all(scripts.as_ref())).unwrap();
            let bus = {
                let _entered = rt.enter();
                EventBus::new(Arc::new(StubEventLog))
            };
            let backend = test_backend_with_scripts(Arc::clone(&scripts) as Arc<dyn ScriptRepo>);
            let handle = rt.handle().clone();
            let shared = Arc::clone(&registry);
            let view = cx.new(|cx| {
                ScriptEditorView::new(backend, shared, bus, Arc::new(NothingDisabled), handle, cx)
            });
            let rig = Self {
                rt,
                view,
                scripts,
                registry,
                id,
            };
            rig.settle(cx);
            rig
        }

        fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..6 {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn runnable(&self, name: &str) -> bool {
            self.rt.block_on(self.registry.get_by_name(name)).is_some()
        }

        fn listed(&self, cx: &mut TestAppContext) -> Vec<String> {
            self.view.read_with(cx, |view, _| {
                view.scripts.iter().map(|e| e.name.clone()).collect()
            })
        }

        fn stage(&self, found: ScriptReferences, cx: &mut TestAppContext) {
            let id = self.id;
            self.view.update(cx, |view, cx| {
                view.stage_rename(
                    PendingRename {
                        id,
                        old_name: OLD_NAME.to_owned(),
                        new_name: NEW_NAME.to_owned(),
                        references: ScriptReferences::default(),
                    },
                    Ok(found),
                    cx,
                );
            });
            self.settle(cx);
        }

        fn prompting_rename(&self, cx: &mut TestAppContext) -> bool {
            self.view
                .read_with(cx, |view, _| view.pending_rename.is_some())
        }
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, String)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|t| (t.kind, t.message.to_string()))
                .collect()
        })
    }

    fn press_undo(cx: &mut TestAppContext) {
        let action = cx.update(|cx| {
            let toasts = cx.global_mut::<Toasts>();
            let id = toasts
                .items()
                .iter()
                .find(|t| t.kind == ToastKind::Undo)
                .map(|t| t.id)
                .expect("an undo toast");
            toasts
                .take(id)
                .and_then(|t| t.action)
                .expect("an undo action")
        });
        let window: AnyWindowHandle = cx.add_window(|_, _| Blank).into();
        cx.update(|cx| {
            cx.update_window(window, |_, window, cx| (action.on_action)(window, cx))
                .unwrap();
        });
    }

    #[gpui::test]
    fn a_rename_no_action_uses_applies_at_once_and_a_used_one_waits_for_confirmation(
        cx: &mut TestAppContext,
    ) {
        for (uses, prompted, stored, case) in [
            (references(&[], &[]), false, NEW_NAME, "unused"),
            (
                references(&["Greeter"], &[]),
                true,
                OLD_NAME,
                "named by an action",
            ),
            (
                references(&[], &["Picker"]),
                true,
                OLD_NAME,
                "picked by a variable",
            ),
        ] {
            let rig = Rig::mount(cx, uses.clone());

            rig.stage(uses, cx);

            assert_eq!(
                (rig.prompting_rename(cx), rig.scripts.live_names()),
                (prompted, vec![stored.to_owned()]),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn confirming_a_staged_rename_renames_the_script_everywhere_and_reports_the_updated_actions(
        cx: &mut TestAppContext,
    ) {
        let uses = references(&["Greeter", "Welcomer"], &["Picker"]);
        let rig = Rig::mount(cx, uses.clone());
        rig.stage(uses, cx);

        rig.view
            .update(cx, |view, cx| view.confirm_pending_rename(cx));
        rig.settle(cx);

        let raised = toasts(cx);
        assert_eq!(
            (
                rig.scripts.live_names(),
                rig.listed(cx),
                rig.runnable(NEW_NAME),
                rig.runnable(OLD_NAME),
                raised.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            ),
            (
                vec![NEW_NAME.to_owned()],
                vec![NEW_NAME.to_owned()],
                true,
                false,
                vec![ToastKind::Success, ToastKind::Warn],
            )
        );
        assert!(
            raised[0].1.contains('2') && raised[1].1.contains("Picker"),
            "{raised:?}"
        );
    }

    #[gpui::test]
    fn cancelling_a_staged_rename_keeps_the_old_name(cx: &mut TestAppContext) {
        let uses = references(&["Greeter"], &[]);
        let rig = Rig::mount(cx, uses.clone());
        rig.stage(uses, cx);

        rig.view
            .update(cx, |view, cx| view.cancel_pending_rename(cx));
        rig.settle(cx);

        assert_eq!(
            (
                rig.prompting_rename(cx),
                rig.scripts.live_names(),
                rig.runnable(OLD_NAME)
            ),
            (false, vec![OLD_NAME.to_owned()], true)
        );
    }

    #[gpui::test]
    fn the_delete_prompt_names_every_action_that_uses_the_script(cx: &mut TestAppContext) {
        let rig = Rig::mount(cx, references(&["Greeter", "Welcomer"], &["Picker"]));
        let id = rig.id;

        rig.view.update(cx, |view, cx| view.request_delete(id, cx));
        rig.settle(cx);

        let hint = rig
            .view
            .read_with(cx, |view, _| delete_hint(view.delete_referrers.as_ref()));
        for name in ["Greeter", "Welcomer", "Picker"] {
            assert!(hint.contains(name), "{name} missing from {hint:?}");
        }
    }

    #[gpui::test]
    fn deleting_archives_the_script_out_of_reach_and_undo_brings_it_back(cx: &mut TestAppContext) {
        let rig = Rig::mount(cx, ScriptReferences::default());
        let id = rig.id;
        rig.view.update(cx, |view, cx| view.request_delete(id, cx));
        rig.settle(cx);

        rig.view.update(cx, |view, cx| view.confirm_delete(cx));
        rig.settle(cx);
        let deleted = (
            rig.scripts.archived_ids(),
            rig.listed(cx),
            rig.runnable(OLD_NAME),
        );
        press_undo(cx);
        rig.settle(cx);
        let undone = (
            rig.scripts.archived_ids(),
            rig.listed(cx),
            rig.runnable(OLD_NAME),
        );

        assert_eq!(
            (deleted, undone),
            (
                (vec![id], Vec::new(), false),
                (Vec::new(), vec![OLD_NAME.to_owned()], true)
            )
        );
    }
}

fn rejection(kind: VariantKind, raw: &str, language: forge_storage::Language) -> String {
    crate::i18n::install_language(language);
    without_isolates(
        &parse_input_to_variant("amount", kind, raw)
            .unwrap_err()
            .user_message(),
    )
}

fn without_isolates(text: &str) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

#[test]
fn a_rejected_script_input_reads_in_the_active_language() {
    use forge_storage::Language::{En, Uk};

    for (kind, raw, en, uk) in [
        (
            VariantKind::Int,
            "x",
            "`amount` must be an integer".to_owned(),
            "`amount` має бути цілим числом".to_owned(),
        ),
        (
            VariantKind::Float,
            "x",
            "`amount` must be a number".to_owned(),
            "`amount` має бути числом".to_owned(),
        ),
        (
            VariantKind::Bool,
            "x",
            "`amount` must be `true` or `false`".to_owned(),
            "`amount` має бути `true` або `false`".to_owned(),
        ),
        (
            VariantKind::Datetime,
            "x",
            format!("`amount` must be an RFC3339 timestamp, e.g. `{DATETIME_INPUT_EXAMPLE}`"),
            format!(
                "`amount` має бути часовою позначкою RFC3339, наприклад `{DATETIME_INPUT_EXAMPLE}`"
            ),
        ),
        (
            VariantKind::Array,
            "{}",
            format!("`amount` must be a JSON array, e.g. `{ARRAY_INPUT_EXAMPLE}`"),
            format!("`amount` має бути масивом JSON, наприклад `{ARRAY_INPUT_EXAMPLE}`"),
        ),
        (
            VariantKind::Object,
            "x",
            format!("`amount` must be a JSON object, e.g. `{OBJECT_INPUT_EXAMPLE}`"),
            format!("`amount` має бути об'єктом JSON, наприклад `{OBJECT_INPUT_EXAMPLE}`"),
        ),
    ] {
        assert_eq!(rejection(kind, raw, En), en, "{kind:?} en");
        assert_eq!(rejection(kind, raw, Uk), uk, "{kind:?} uk");
    }
}

#[test]
fn an_invalid_input_reason_is_shown_unchanged_in_either_language() {
    for language in [forge_storage::Language::En, forge_storage::Language::Uk] {
        crate::i18n::install_language(language);

        let shown = ScriptInputRejection::new(
            "amount",
            ScriptInputReason::Invalid("float is not finite".to_owned()),
        )
        .user_message();

        assert_eq!(
            without_isolates(&shown),
            "`amount`: float is not finite",
            "{language:?}"
        );
    }
}
