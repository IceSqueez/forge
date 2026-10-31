use super::clients_section::SUBSCRIBE_ALL_LABEL;
use super::overlay_section::{OverlayListing, is_html_path, scan_overlay_root};
use super::polling::{
    ServerPoll, client_row_from_snapshot, count_recent_clients, subscription_chip,
};
use super::*;
use crate::async_bridge;
use forge_components::ToastKind;
use forge_overlay::MARKUP_FILE;
use forge_overlay::RESERVED_DIRECTORY;
use forge_server::BandwidthSnapshot;
use forge_server::{ConnectedClientSnapshot, EventFilterSnapshot, ServerSnapshot};
use gpui::{Entity, TestAppContext};

use crate::i18n::install_language;
use crate::test_support::{
    install_presentation, pump_until, runtime, stopped_server_hosting, test_backend,
};
use forge_storage::Language;

const WILDCARD_BIND: &str = "0.0.0.0:9515";
const ROUTABLE_HOST: &str = "192.168.1.5";
const OVERLAY_DIR: &str = "alerts";
const PLAIN_DIR: &str = "assets";
const LOOSE_PAGE: &str = "legacy.html";
const OVERLAY_TOKEN: &str = "fgo_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde";
const ROTATED_TOKEN: &str = "fgo_ZyXwVuTsRqPoNmLkJiHgFeDcBa9876543210_-edcba";

fn serverless_console(
    cx: &mut TestAppContext,
    rt: &tokio::runtime::Runtime,
) -> Entity<ServerConsoleView> {
    let (backend, _writes) = test_backend();
    cx.update(|cx| crate::overlay_link_token::publish(forge_types::Secret::new(OVERLAY_TOKEN), cx));
    cx.new(|cx| {
        ServerConsoleView::new(
            None,
            rt.handle().clone(),
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
            backend as Arc<dyn SettingsRepo>,
            cx,
        )
    })
}

fn dir_entry(name: &str, overlay: bool) -> OwnedOverlayEntry {
    OwnedOverlayEntry {
        name: name.to_owned(),
        kind: OwnedOverlayKind::Dir { overlay },
        size_bytes: 0,
        child_count: 1,
    }
}

fn file_entry(name: &str) -> OwnedOverlayEntry {
    OwnedOverlayEntry {
        name: name.to_owned(),
        kind: OwnedOverlayKind::File {
            html: is_html_path(std::path::Path::new(name)),
        },
        size_bytes: 512,
        child_count: 0,
    }
}

fn serve(view: &mut ServerConsoleView, entries: Vec<OwnedOverlayEntry>) {
    view.running = true;
    view.bind_address = Some(WILDCARD_BIND.to_owned());
    view.routable_host = Some(ROUTABLE_HOST.to_owned());
    view.overlay_entries = entries;
}

fn listing_poll(entries: Vec<OwnedOverlayEntry>) -> ServerPoll {
    ServerPoll {
        snapshot: ServerSnapshot {
            uptime_seconds: 0,
            connected_clients: Vec::new(),
            bandwidth: BandwidthSnapshot {
                outbound_bytes_per_second: 0,
                outbound_bytes_total: 0,
                peak_outbound_bytes_per_second: 0,
            },
            aggregate_events_per_second: 0.0,
            http_requests_total: 0,
            events_out_total: 0,
            dropped_events_total: 0,
        },
        bind_address: Some(WILDCARD_BIND.to_owned()),
        overlay: Some(OverlayListing {
            root: "/overlays".to_owned(),
            entries,
        }),
    }
}

#[gpui::test]
fn the_default_browser_source_is_an_overlay_directory_ahead_of_a_loose_page(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    let url = view.update(cx, |this, _| {
        serve(
            this,
            vec![
                dir_entry(PLAIN_DIR, false),
                dir_entry(OVERLAY_DIR, true),
                file_entry(LOOSE_PAGE),
            ],
        );
        this.browser_source_url()
    });

    assert_eq!(
        url.as_deref(),
        Some(format!("http://192.168.1.5:9515/overlays/{OVERLAY_TOKEN}/alerts/").as_str())
    );
}

#[gpui::test]
fn a_picked_file_replaces_the_default_directory_and_is_addressed_without_a_slash(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    let url = view.update(cx, |this, cx| {
        serve(
            this,
            vec![dir_entry(OVERLAY_DIR, true), file_entry(LOOSE_PAGE)],
        );
        this.select_overlay_entry(LOOSE_PAGE.to_owned(), cx);
        this.browser_source_url()
    });

    assert_eq!(
        url.as_deref(),
        Some(format!("http://192.168.1.5:9515/overlays/{OVERLAY_TOKEN}/legacy.html").as_str())
    );
}

#[gpui::test]
fn a_directory_without_an_entry_document_cannot_become_the_browser_source(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    for unhostable in [PLAIN_DIR, RESERVED_DIRECTORY] {
        let url = view.update(cx, |this, cx| {
            serve(
                this,
                vec![dir_entry(unhostable, false), dir_entry(OVERLAY_DIR, true)],
            );
            this.select_overlay_entry(unhostable.to_owned(), cx);
            this.browser_source_url()
        });

        assert_eq!(
            url.as_deref(),
            Some(format!("http://192.168.1.5:9515/overlays/{OVERLAY_TOKEN}/alerts/").as_str()),
            "directory {unhostable}"
        );
    }
}

#[gpui::test]
fn a_picked_entry_stays_the_browser_source_across_rescans_that_shift_its_position(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    for (rescan, case) in [
        (
            vec![
                file_entry("intro.html"),
                dir_entry(OVERLAY_DIR, true),
                file_entry(LOOSE_PAGE),
            ],
            "entry inserted ahead of the pick",
        ),
        (
            vec![file_entry(LOOSE_PAGE), dir_entry(OVERLAY_DIR, true)],
            "listing reordered",
        ),
    ] {
        let url = view.update(cx, |this, cx| {
            serve(
                this,
                vec![dir_entry(OVERLAY_DIR, true), file_entry(LOOSE_PAGE)],
            );
            this.select_overlay_entry(LOOSE_PAGE.to_owned(), cx);
            this.apply_poll(listing_poll(rescan));
            this.browser_source_url()
        });

        assert_eq!(
            url.as_deref(),
            Some(format!("http://192.168.1.5:9515/overlays/{OVERLAY_TOKEN}/legacy.html").as_str()),
            "{case}"
        );
    }
}

#[gpui::test]
fn a_pick_whose_entry_disappears_from_a_rescan_falls_back_to_the_default_source(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    let url = view.update(cx, |this, cx| {
        serve(
            this,
            vec![
                dir_entry(OVERLAY_DIR, true),
                file_entry(LOOSE_PAGE),
                file_entry("spare.html"),
            ],
        );
        this.select_overlay_entry("spare.html".to_owned(), cx);
        this.apply_poll(listing_poll(vec![
            file_entry(LOOSE_PAGE),
            dir_entry(OVERLAY_DIR, true),
        ]));
        this.browser_source_url()
    });

    assert_eq!(
        url.as_deref(),
        Some(format!("http://192.168.1.5:9515/overlays/{OVERLAY_TOKEN}/alerts/").as_str())
    );
}

#[gpui::test]
fn copying_the_browser_source_copies_the_picked_entry_after_a_rescan(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);
    cx.update(|cx| cx.set_global(forge_components::Toasts::new()));

    view.update(cx, |this, cx| {
        serve(
            this,
            vec![dir_entry(OVERLAY_DIR, true), file_entry(LOOSE_PAGE)],
        );
        this.select_overlay_entry(LOOSE_PAGE.to_owned(), cx);
        this.apply_poll(listing_poll(vec![
            file_entry("intro.html"),
            dir_entry(OVERLAY_DIR, true),
            file_entry(LOOSE_PAGE),
        ]));
        this.copy_browser_source_url(cx);
    });

    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(format!(
            "http://192.168.1.5:9515/overlays/{OVERLAY_TOKEN}/legacy.html"
        ))
    );
}

#[gpui::test]
fn no_browser_source_url_is_offered_until_the_server_hosts_something(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    for (running, entries, case) in [
        (false, vec![dir_entry(OVERLAY_DIR, true)], "server stopped"),
        (true, Vec::new(), "overlay root empty"),
        (
            true,
            vec![dir_entry(PLAIN_DIR, false)],
            "nothing hostable in the overlay root",
        ),
    ] {
        let url = view.update(cx, |this, _| {
            serve(this, entries);
            this.running = running;
            this.browser_source_url()
        });

        assert_eq!(url, None, "{case}");
    }
}

#[gpui::test]
fn the_browser_source_url_follows_a_regenerated_overlay_token(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);
    view.update(cx, |this, _| {
        serve(this, vec![dir_entry(OVERLAY_DIR, true)])
    });

    cx.update(|cx| crate::overlay_link_token::publish(forge_types::Secret::new(ROTATED_TOKEN), cx));
    cx.run_until_parked();

    let url = view.update(cx, |this, _| this.browser_source_url());
    assert_eq!(
        url.as_deref(),
        Some(format!("http://192.168.1.5:9515/overlays/{ROTATED_TOKEN}/alerts/").as_str())
    );
}

#[gpui::test]
fn the_displayed_browser_source_url_never_shows_the_full_overlay_token(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    let shown = view.update(cx, |this, _| {
        serve(this, vec![dir_entry(OVERLAY_DIR, true)]);
        this.displayed_browser_source_url()
    });

    let shown = shown.expect("a hosted overlay is offered as a browser source");
    assert!(!shown.contains(OVERLAY_TOKEN), "shown {shown}");
    assert!(shown.ends_with("/alerts/"), "shown {shown}");
}

#[gpui::test]
fn no_browser_source_url_is_offered_before_the_overlay_token_is_known(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    let url = view.update(cx, |this, _| {
        serve(this, vec![dir_entry(OVERLAY_DIR, true)]);
        this.overlay_token = None;
        this.browser_source_url()
    });

    assert_eq!(url, None);
}

fn snapshot_client(
    identification: &str,
    events_per_second: f32,
    uptime_seconds: i64,
) -> ConnectedClientSnapshot {
    ConnectedClientSnapshot {
        id: ClientId::next(),
        identification: identification.to_owned(),
        remote_addr: "203.0.113.10".to_owned(),
        client_type: "obs_browser".to_owned(),
        subscriptions: Vec::new(),
        events_per_second,
        uptime_seconds,
    }
}

fn filter(source: &str, kind: &str) -> EventFilterSnapshot {
    EventFilterSnapshot {
        source: source.to_owned(),
        kind: kind.to_owned(),
    }
}

#[gpui::test]
fn restart_is_offered_only_while_the_server_runs_with_no_restart_in_flight(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    for (running, restarting, enabled, disabled, case) in [
        (true, false, true, false, "a running, idle server"),
        (true, true, true, true, "a restart already in flight"),
        (false, false, true, true, "a stopped server"),
        (false, true, true, true, "a restart in flight while stopped"),
        (
            true,
            false,
            false,
            false,
            "a server still serving after settings switched it off",
        ),
        (
            false,
            false,
            false,
            true,
            "a stopped server that settings switched off",
        ),
    ] {
        view.update(cx, |this, _| {
            this.running = running;
            this.restarting = restarting;
            this.enabled = enabled;
            assert_eq!(this.restart_disabled(), disabled, "{case}");
        });
    }
}

#[gpui::test]
fn the_bind_field_shows_an_address_only_while_the_server_runs(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    for (running, bind_address, expected, case) in [
        (
            true,
            Some(WILDCARD_BIND),
            BindState::Bound(WILDCARD_BIND.to_owned()),
            "running with its address known",
        ),
        (
            true,
            None,
            BindState::AwaitingAddress,
            "running before the first poll answers",
        ),
        (false, None, BindState::Stopped, "stopped"),
        (
            false,
            Some(WILDCARD_BIND),
            BindState::Stopped,
            "stopped with a stale address still held",
        ),
    ] {
        view.update(cx, |this, _| {
            this.running = running;
            this.bind_address = bind_address.map(str::to_owned);
            assert_eq!(this.bind_state(), expected, "{case}");
        });
    }
}

#[test]
fn scan_overlay_root_puts_directories_first_and_flags_only_those_with_an_entry_document() {
    const PAGE: &[u8] = b"<!doctype html>";
    const NOTE: &[u8] = b"notes";
    let root = tempfile::tempdir().expect("tempdir");
    let hosted = root.path().join("alerts");
    let plain = root.path().join("Assets");
    std::fs::create_dir(&hosted).expect("hosted dir");
    std::fs::create_dir(&plain).expect("plain dir");
    std::fs::create_dir(root.path().join(RESERVED_DIRECTORY)).expect("reserved dir");
    std::fs::write(hosted.join(MARKUP_FILE), PAGE).expect("entry document");
    std::fs::write(hosted.join("alerts.js"), b"").expect("sibling asset");
    std::fs::write(plain.join("style.css"), b"").expect("plain asset");
    std::fs::write(root.path().join("legacy.html"), PAGE).expect("loose page");
    std::fs::write(root.path().join("Notes.txt"), NOTE).expect("loose file");

    let listing = runtime().block_on(scan_overlay_root(root.path()));

    let listed: Vec<(&str, OwnedOverlayKind, u32, u64)> = listing
        .entries
        .iter()
        .map(|entry| {
            (
                entry.name.as_str(),
                entry.kind,
                entry.child_count,
                entry.size_bytes,
            )
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            ("alerts", OwnedOverlayKind::Dir { overlay: true }, 2, 0),
            ("Assets", OwnedOverlayKind::Dir { overlay: false }, 1, 0),
            (
                RESERVED_DIRECTORY,
                OwnedOverlayKind::Dir { overlay: false },
                0,
                0
            ),
            (
                "legacy.html",
                OwnedOverlayKind::File { html: true },
                0,
                PAGE.len() as u64
            ),
            (
                "Notes.txt",
                OwnedOverlayKind::File { html: false },
                0,
                NOTE.len() as u64
            ),
        ]
    );
}

#[test]
fn scan_overlay_root_yields_an_empty_listing_for_a_root_it_cannot_read() {
    let root = tempfile::tempdir().expect("tempdir");

    let listing = runtime().block_on(scan_overlay_root(&root.path().join("not-created")));

    assert!(
        listing.entries.is_empty(),
        "a missing overlay root must degrade to an empty picker, not break the poll"
    );
}

#[test]
fn is_html_path_accepts_html_and_htm_in_any_case() {
    for name in ["a.html", "a.htm", "a.HTML", "a.Htm"] {
        assert!(is_html_path(std::path::Path::new(name)), "{name}");
    }
    for name in ["a.htmlx", "a.txt", "a.js", "html", "a."] {
        assert!(!is_html_path(std::path::Path::new(name)), "{name}");
    }
}

#[test]
fn count_recent_clients_counts_only_the_last_ten_minutes() {
    let clients = [
        snapshot_client("stale", 0.0, RECENT_CLIENT_WINDOW_SECONDS + 1),
        snapshot_client("edge", 0.0, RECENT_CLIENT_WINDOW_SECONDS),
        snapshot_client("fresh", 0.0, 0),
        snapshot_client("skewed", 0.0, -1),
    ];
    assert_eq!(count_recent_clients(&clients), 2);
}

#[test]
fn client_row_marks_a_silent_client_idle_and_an_emitting_one_active() {
    for (eps, expected) in [
        (0.0, ClientLiveness::Idle),
        (0.05, ClientLiveness::Active),
        (12.5, ClientLiveness::Active),
    ] {
        let row = client_row_from_snapshot(&snapshot_client("dash", eps, 30));
        assert_eq!(row.liveness, expected, "eps {eps}");
    }
}

#[test]
fn client_rows_sharing_a_remote_ip_still_get_distinct_element_keys() {
    let first = client_row_from_snapshot(&snapshot_client("127.0.0.1:51001", 0.0, 30));
    let second = client_row_from_snapshot(&snapshot_client("127.0.0.1:51002", 0.0, 30));
    assert_ne!(
        first.key, second.key,
        "two overlays on one host must not collide on the disconnect button id"
    );
}

#[test]
fn client_row_clamps_a_negative_uptime_to_zero() {
    crate::i18n::install_language(forge_storage::Language::En);
    let row = client_row_from_snapshot(&snapshot_client("dash", 0.0, -42));
    assert_eq!(
        row.uptime_short
            .replace(crate::test_support::FORMAT_ISOLATES, ""),
        "0s"
    );
}

#[test]
fn subscription_chip_collapses_a_full_wildcard_to_a_single_label() {
    let chip = subscription_chip(&filter("*", "*"));
    assert_eq!(chip.label, SUBSCRIBE_ALL_LABEL);
    assert_eq!(chip.kind, SubscriptionChipKind::All);
}

#[test]
fn subscription_chip_labels_each_source_and_kind_combination() {
    for (source, kind, expected_label, expected_kind) in [
        (
            "twitch",
            "chat.message",
            "twitch.chat.message",
            SubscriptionChipKind::Source(EventSource::Twitch),
        ),
        (
            "twitch",
            "*",
            "twitch.*",
            SubscriptionChipKind::Source(EventSource::Twitch),
        ),
        (
            "youtube",
            "*",
            "youtube.*",
            SubscriptionChipKind::Source(EventSource::YouTube),
        ),
        (
            "vtube",
            "model.loaded",
            "vtube.model.loaded",
            SubscriptionChipKind::Source(EventSource::VTube),
        ),
        (
            "*",
            "chat.message",
            "chat.message",
            SubscriptionChipKind::Unknown,
        ),
        (
            "peacock",
            "chat.message",
            "peacock.chat.message",
            SubscriptionChipKind::Unknown,
        ),
    ] {
        let chip = subscription_chip(&filter(source, kind));
        assert_eq!(chip.label, expected_label, "source {source} kind {kind}");
        assert_eq!(chip.kind, expected_kind, "source {source} kind {kind}");
    }
}

struct HostingConsole {
    view: Entity<ServerConsoleView>,
    root: tempfile::TempDir,
    rt: tokio::runtime::Runtime,
}

fn console_hosting_a_folder(cx: &mut TestAppContext) -> HostingConsole {
    install_language(Language::En);
    install_presentation(cx);
    cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
    let rt = runtime();
    let root = tempfile::tempdir().expect("tempdir");
    let (backend, _writes) = test_backend();
    let server = rt.block_on(stopped_server_hosting(&backend, root.path().to_path_buf()));
    let handle = rt.handle().clone();
    let view = cx.new(|cx| {
        ServerConsoleView::new(
            Some(server),
            handle,
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
            backend as Arc<dyn SettingsRepo>,
            cx,
        )
    });
    HostingConsole { view, root, rt }
}

impl HostingConsole {
    fn listed(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            view.overlay_entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect()
        })
    }

    fn seat_namesake_clients(&self, cx: &mut TestAppContext) -> (ClientId, ClientId) {
        self.view.update(cx, |view, _| {
            view.connected_clients = vec![
                client_row_from_snapshot(&snapshot_client("obs-browser", 0.0, 30)),
                client_row_from_snapshot(&snapshot_client("obs-browser", 0.0, 30)),
            ];
            (view.connected_clients[0].id, view.connected_clients[1].id)
        })
    }

    fn seated_ids(&self, cx: &mut TestAppContext) -> Vec<ClientId> {
        self.view.read_with(cx, |view, _| {
            view.connected_clients.iter().map(|row| row.id).collect()
        })
    }

    fn refresh_until(
        &self,
        cx: &mut TestAppContext,
        mut reached: impl FnMut(&mut TestAppContext) -> bool,
    ) {
        self.view
            .update(cx, |view, cx| view.refresh_overlay_listing(cx));
        pump_until(&self.rt, || {
            cx.run_until_parked();
            reached(cx)
        });
    }
}

#[gpui::test]
fn confirming_a_disconnect_drops_only_the_chosen_row_of_two_namesakes(cx: &mut TestAppContext) {
    let console = console_hosting_a_folder(cx);
    let (first, second) = console.seat_namesake_clients(cx);

    console.view.update(cx, |view, cx| {
        view.request_disconnect(1, cx);
        view.confirm_disconnect(cx);
    });
    pump_until(&console.rt, || {
        cx.run_until_parked();
        console.seated_ids(cx).len() == 1
    });

    assert_eq!(console.seated_ids(cx), [first]);
    assert_ne!(first, second);
}

#[gpui::test]
fn disconnecting_a_client_the_server_no_longer_holds_says_it_had_already_gone(
    cx: &mut TestAppContext,
) {
    let console = console_hosting_a_folder(cx);
    console.seat_namesake_clients(cx);

    console.view.update(cx, |view, cx| {
        view.request_disconnect(0, cx);
        view.confirm_disconnect(cx);
    });
    pump_until(&console.rt, || {
        cx.run_until_parked();
        console.seated_ids(cx).len() == 1
    });

    let shown = cx.update(|cx| {
        cx.global::<forge_components::Toasts>()
            .items()
            .iter()
            .map(|toast| (toast.kind, toast.message.to_string()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        shown,
        [(ToastKind::Info, tr!("server_client_already_gone"))]
    );
}

#[gpui::test]
fn refreshing_lists_a_file_added_to_the_overlay_folder(cx: &mut TestAppContext) {
    let console = console_hosting_a_folder(cx);
    std::fs::write(console.root.path().join(LOOSE_PAGE), b"<p>").expect("page");

    console.refresh_until(cx, |cx| console.listed(cx) == [LOOSE_PAGE]);
}

#[gpui::test]
fn refreshing_drops_a_picked_entry_that_left_the_overlay_folder(cx: &mut TestAppContext) {
    let console = console_hosting_a_folder(cx);
    let page = console.root.path().join(LOOSE_PAGE);
    std::fs::write(&page, b"<p>").expect("page");
    console.refresh_until(cx, |cx| console.listed(cx) == [LOOSE_PAGE]);
    console.view.update(cx, |view, cx| {
        view.select_overlay_entry(LOOSE_PAGE.to_owned(), cx)
    });

    std::fs::remove_file(&page).expect("page removed");
    console.refresh_until(cx, |cx| console.listed(cx).is_empty());

    let picked = console
        .view
        .read_with(cx, |view, _| view.selected_overlay_entry.clone());
    assert_eq!(picked, None);
}

#[gpui::test]
fn regenerating_the_token_waits_for_the_confirmation(cx: &mut TestAppContext) {
    let console = console_hosting_a_folder(cx);
    let token = |cx: &mut TestAppContext| {
        console
            .view
            .read_with(cx, |view, _| view.bearer_token.expose().to_owned())
    };
    pump_until(&console.rt, || {
        cx.run_until_parked();
        !token(cx).is_empty()
    });
    let original = token(cx);

    console.view.update(cx, |view, cx| {
        view.request_regenerate(cx);
        view.cancel_regenerate(cx);
    });
    crate::test_support::pump(&console.rt);
    cx.run_until_parked();
    let after_cancel = token(cx);
    console.view.update(cx, |view, cx| {
        view.request_regenerate(cx);
        view.confirm_regenerate(cx);
    });
    pump_until(&console.rt, || {
        cx.run_until_parked();
        token(cx) != original
    });

    assert_eq!(after_cancel, original);
}

#[gpui::test]
fn a_poll_that_skipped_the_serving_facts_keeps_the_known_address_and_listing(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    let (resolve, address, entries) = view.update(cx, |this, _| {
        serve(this, vec![dir_entry(OVERLAY_DIR, true)]);
        let mut poll = listing_poll(Vec::new());
        poll.bind_address = None;
        poll.overlay = None;
        let resolve = this.apply_poll(poll);
        (
            resolve,
            this.bind_address.clone(),
            this.overlay_entries.len(),
        )
    });

    assert_eq!(
        (resolve, address, entries),
        (None, Some(WILDCARD_BIND.to_owned()), 1)
    );
}

#[gpui::test]
fn an_unreadable_enabled_setting_keeps_the_state_already_shown(cx: &mut TestAppContext) {
    install_language(Language::En);
    install_presentation(cx);
    let rt = runtime();
    let (backend, _writes) = test_backend();
    let settings = forge_storage::test_support::MemorySettings::new();
    settings.fail_reads(true);
    let view = cx.new(|cx| {
        ServerConsoleView::new(
            None,
            rt.handle().clone(),
            backend as Arc<dyn CredentialsRepo>,
            Arc::new(settings) as Arc<dyn SettingsRepo>,
            cx,
        )
    });

    let settled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let landed = Arc::clone(&settled);
    view.update(cx, |view, cx| {
        view.enabled = false;
        view.fetch_enabled(cx);
        async_bridge::run_async(
            &rt.handle().clone(),
            async {},
            move |_: &mut ServerConsoleView, (), _| {
                landed.store(true, std::sync::atomic::Ordering::SeqCst);
            },
            cx,
        );
    });
    pump_until(&rt, || {
        cx.run_until_parked();
        settled.load(std::sync::atomic::Ordering::SeqCst)
    });

    assert!(!view.read_with(cx, |view, _| view.enabled));
}

#[gpui::test]
fn a_new_bind_address_asks_for_a_host_resolve_once(cx: &mut TestAppContext) {
    let rt = runtime();
    let view = serverless_console(cx, &rt);

    let (first, repeat) = view.update(cx, |this, _| {
        let first = this.apply_poll(listing_poll(Vec::new()));
        this.routable_host_resolved_for.clone_from(&first);
        (first, this.apply_poll(listing_poll(Vec::new())))
    });

    assert_eq!((first, repeat), (Some(WILDCARD_BIND.to_owned()), None));
}
