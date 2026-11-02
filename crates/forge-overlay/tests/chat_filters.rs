mod common;
use common::{CHAT_KIND, text, with_kind};

use forge_overlay::config::{
    AUTHOR, COMMAND_PREFIX, HIDDEN_ACCOUNTS, HIDE_COMMANDS, MESSAGE, POSITION, TEXT_SIZE,
};
use forge_overlay::{
    ChatRowFilter, OverlayConfig, OverlayError, effective_overlay_config, validate_overlay_config,
};
use forge_types::{ArgStack, Variant};

const LOGIN_ARG: &str = "user_login";
const MESSAGE_ARG: &str = "message_text";

fn commands(hide: Option<Variant>, prefix: Option<&str>) -> OverlayConfig {
    hide.map(|hide| (HIDE_COMMANDS.to_owned(), hide))
        .into_iter()
        .chain(prefix.map(|prefix| (COMMAND_PREFIX.to_owned(), text(prefix))))
        .collect()
}

fn accounts(listed: &str) -> OverlayConfig {
    OverlayConfig::from([(HIDDEN_ACCOUNTS.to_owned(), text(listed))])
}

fn shown(author: &str, message: &str) -> OverlayConfig {
    OverlayConfig::from([
        (AUTHOR.to_owned(), text(author)),
        (MESSAGE.to_owned(), text(message)),
    ])
}

fn run_args(login: &str, message: &str) -> ArgStack {
    ArgStack::new()
        .set(LOGIN_ARG.to_owned(), text(login))
        .set(MESSAGE_ARG.to_owned(), text(message))
}

fn withholds(stored: &OverlayConfig, delivered: &OverlayConfig, args: &ArgStack) -> bool {
    with_kind(CHAT_KIND, |descriptor| {
        descriptor.withholds(stored, delivered, args)
    })
}

fn stored_before_the_filters() -> OverlayConfig {
    OverlayConfig::from([
        (POSITION.to_owned(), text("top")),
        (TEXT_SIZE.to_owned(), Variant::Int(20)),
    ])
}

#[test]
fn a_command_is_hidden_only_while_the_toggle_is_on_and_the_prefix_is_set() {
    let on = Some(Variant::Bool(true));
    for (config, message, hidden, case) in [
        (
            commands(on.clone(), Some("!")),
            "!so ada",
            true,
            "toggle on",
        ),
        (
            commands(on.clone(), Some("!")),
            "  \t!so ada",
            true,
            "leading whitespace before the prefix",
        ),
        (
            commands(Some(Variant::Bool(false)), Some("!")),
            "!so ada",
            false,
            "toggle off",
        ),
        (commands(None, Some("!")), "!so ada", false, "toggle unset"),
        (
            commands(Some(text("true")), Some("!")),
            "!so ada",
            false,
            "toggle held as text",
        ),
        (
            commands(on.clone(), Some("!")),
            "hello !so",
            false,
            "prefix mid-message",
        ),
        (
            commands(on.clone(), Some("")),
            "!so ada",
            false,
            "empty prefix",
        ),
        (
            commands(on.clone(), Some("")),
            "hello",
            false,
            "empty prefix on plain text",
        ),
        (
            commands(on.clone(), Some("   ")),
            "   hello",
            false,
            "blank prefix",
        ),
        (
            commands(on.clone(), None),
            "!so ada",
            false,
            "prefix absent",
        ),
        (
            commands(on.clone(), Some(" ?! ")),
            "?!lurk",
            true,
            "padded multi-char prefix",
        ),
        (
            commands(on, Some("?!")),
            "?lurk",
            false,
            "half of a multi-char prefix",
        ),
    ] {
        assert_eq!(
            ChatRowFilter::from_config(&config).hides("viewer", message),
            hidden,
            "{case}: {message:?}"
        );
    }
}

#[test]
fn a_listed_account_is_hidden_whatever_its_case_sigil_or_separator() {
    for (listed, login, hidden) in [
        ("Nightbot", "nightbot", true),
        ("nightbot", "NightBot", true),
        ("@Nightbot", "nightbot", true),
        ("nightbot", "@nightbot", true),
        ("nightbot,streamelements", "StreamElements", true),
        ("nightbot streamelements", "streamelements", true),
        ("nightbot, @StreamElements", "streamelements", true),
        ("  nightbot ,\t\nmoobot  ", "moobot", true),
        ("nightbot", "  nightbot  ", true),
        ("nightbot", "nightbot2", false),
        ("nightbot2", "nightbot", false),
        ("night bot", "nightbot", false),
        ("nightbot", "viewer", false),
        ("", "nightbot", false),
    ] {
        assert_eq!(
            ChatRowFilter::from_config(&accounts(listed)).hides(login, "hello"),
            hidden,
            "{login:?} against {listed:?}"
        );
    }
}

#[test]
fn an_empty_login_is_never_hidden_even_by_a_list_of_blank_entries() {
    for listed in ["", ",", " , ,", "@", "@, @ ,", "nightbot, ,@"] {
        let filter = ChatRowFilter::from_config(&accounts(listed));
        for login in ["", "   ", "@"] {
            assert!(
                !filter.hides(login, "hello"),
                "{login:?} matched the blank entries of {listed:?}"
            );
        }
    }
}

#[test]
fn hidden_accounts_apply_with_the_command_toggle_off() {
    let mut config = commands(Some(Variant::Bool(false)), Some("!"));
    config.extend(accounts("nightbot"));

    assert!(ChatRowFilter::from_config(&config).hides("nightbot", "plain words"));
}

#[test]
fn the_run_variables_decide_over_the_shown_author_and_message() {
    let mut stored = accounts("nightbot");
    stored.insert(HIDE_COMMANDS.to_owned(), Variant::Bool(true));
    for (delivered, args, withheld, case) in [
        (
            shown("nightbot", "hello"),
            run_args("viewer", "hello"),
            false,
            "a hidden name shown for a visible sender",
        ),
        (
            shown("viewer", "hello"),
            run_args("nightbot", "hello"),
            true,
            "a visible name shown for a hidden sender",
        ),
        (
            shown("viewer", "!so ada"),
            run_args("viewer", "hello"),
            false,
            "a command shown for a plain message",
        ),
        (
            shown("viewer", "hello"),
            run_args("viewer", "!so ada"),
            true,
            "a plain message shown for a command",
        ),
    ] {
        assert_eq!(withholds(&stored, &delivered, &args), withheld, "{case}");
    }
}

#[test]
fn without_run_variables_the_shown_author_and_message_decide() {
    let mut stored = accounts("nightbot");
    stored.insert(HIDE_COMMANDS.to_owned(), Variant::Bool(true));
    for (delivered, withheld) in [
        (shown("Nightbot", "hello"), true),
        (shown("viewer", "!so ada"), true),
        (shown("viewer", "hello"), false),
        (OverlayConfig::new(), false),
    ] {
        assert_eq!(
            withholds(&stored, &delivered, &ArgStack::new()),
            withheld,
            "{delivered:?}"
        );
    }
}

#[test]
fn an_unset_command_prefix_falls_back_to_the_default_when_withholding() {
    let stored = commands(Some(Variant::Bool(true)), None);

    assert!(withholds(
        &stored,
        &shown("viewer", "!so ada"),
        &ArgStack::new()
    ));
}

#[test]
fn a_chat_stored_before_the_filters_hides_nothing_with_the_bang_prefix_ready() {
    let effective = with_kind(CHAT_KIND, |descriptor| {
        effective_overlay_config(descriptor, &stored_before_the_filters())
    });

    assert_eq!(
        [
            effective.get(HIDE_COMMANDS),
            effective.get(COMMAND_PREFIX),
            effective.get(HIDDEN_ACCOUNTS),
        ],
        [
            Some(&Variant::Bool(false)),
            Some(&text("!")),
            Some(&text("")),
        ]
    );
}

#[test]
fn a_chat_stored_before_the_filters_still_validates() {
    let refusal = with_kind(CHAT_KIND, |descriptor| {
        validate_overlay_config(descriptor, &stored_before_the_filters()).err()
    });

    assert!(refusal.is_none(), "{refusal:?}");
}

#[test]
fn a_filter_setting_held_as_the_wrong_type_is_refused() {
    for (key, stored) in [
        (HIDE_COMMANDS, text("true")),
        (HIDE_COMMANDS, Variant::Int(1)),
        (COMMAND_PREFIX, Variant::Bool(true)),
        (HIDDEN_ACCOUNTS, Variant::Int(0)),
    ] {
        let err = with_kind(CHAT_KIND, |descriptor| {
            validate_overlay_config(
                descriptor,
                &OverlayConfig::from([(key.to_owned(), stored.clone())]),
            )
            .expect_err("a value of the wrong type must be refused")
        });

        assert!(
            matches!(&err, OverlayError::WrongType { key: k, .. } if k == key),
            "{key} held as {stored:?} produced {err:?}"
        );
    }
}
