use std::borrow::Cow;
use std::io;
use std::sync::LazyLock;

use regex::Regex;
use tracing::Metadata;
use tracing_subscriber::fmt::MakeWriter;

const JWT: &str = r"\beyJ[A-Za-z0-9_\-]{6,}\.[A-Za-z0-9_\-]{6,}\.[A-Za-z0-9_\-]{4,}";

const BEARER_HEADER: &str = r"(?i)\b(bearer)\s+[A-Za-z0-9._\-~+/]{8,}={0,2}";

const OAUTH_PREFIXED: &str = r"(?i)\b(oauth):[A-Za-z0-9._\-]{8,}";

const VENDOR_KEY: &str = r"(?x)
    \b(?:
        sk-[A-Za-z0-9_\-]{16,}
      | sk_[A-Za-z0-9]{24,}
      | xox[baprs]-[A-Za-z0-9\-]{10,}
      | AIza[A-Za-z0-9_\-]{30,}
      | gh[pousr]_[A-Za-z0-9]{20,}
      | github_pat_[A-Za-z0-9_]{20,}
      | (?:AKIA|ASIA)[0-9A-Z]{16}
    )";

const URL_QUERY: &str = r#"([a-zA-Z][a-zA-Z0-9+.\-]*://[^\s"'<>?]*)\?[^\s"'<>]+"#;

const CREDENTIAL_ASSIGNMENT: &str = r#"(?xi)
    (
        (?: (?:\x1b\[[0-9;]*m)+ | \b )
        (?:
            access_token | refresh_token | client_secret | id_token
          | api[_\-]?key | apikey | authorization | token | secret
          | password | passwd | pwd
        )
        \b
    )
    (
        (?:\x1b\[[0-9;]*m)* "? \s* [:=] \s* (?:\x1b\[[0-9;]*m)*
    )
    "? [A-Za-z0-9._\-~+/]{6,} ={0,2} "?
"#;

const LOG_BRIDGE_FIELD: &str = r#"[ \t]*\blog\.(?:file|line|module_path|target)=(?:"[^"]*"|\S+)"#;

const DEPENDENCY_PATH: &str = r#"(?x)
    [^\s"',;()\[\]{}=]* [/\\] \. (?:cargo|rustup) [/\\] [^\s"',;)\]}]*
    | /rustc/ [0-9a-f]{7,} / [^\s"',;)\]}]*
"#;

const HOME_DIR_UNIX: &str = r#"(?:/home/|/Users/)[^/\\\s:"',;)\]}]+"#;

const HOME_DIR_WINDOWS: &str = r#"(?i)[a-z]:\\Users\\[^\\\s:"',;)\]}]+"#;

const EMAIL: &str = r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b";

const AS_TOKEN: &str = "<redacted:token>";
const AS_BEARER: &str = "${1} <redacted:token>";
const AS_OAUTH: &str = "${1}:<redacted:token>";
const AS_QUERY: &str = "${1}?<redacted:query>";
const AS_ASSIGNED_TOKEN: &str = "${1}${2}<redacted:token>";
const AS_HOME: &str = "~";
const AS_EMAIL: &str = "<redacted:email>";
const AS_NOTHING: &str = "";
const AS_DEPENDENCY: &str = "<dependency>";

const UNAVAILABLE: &str = "<redacted:log scrubber unavailable>";

const VOCABULARY: [(&str, &str); 11] = [
    (LOG_BRIDGE_FIELD, AS_NOTHING),
    (DEPENDENCY_PATH, AS_DEPENDENCY),
    (JWT, AS_TOKEN),
    (BEARER_HEADER, AS_BEARER),
    (OAUTH_PREFIXED, AS_OAUTH),
    (VENDOR_KEY, AS_TOKEN),
    (URL_QUERY, AS_QUERY),
    (CREDENTIAL_ASSIGNMENT, AS_ASSIGNED_TOKEN),
    (HOME_DIR_UNIX, AS_HOME),
    (HOME_DIR_WINDOWS, AS_HOME),
    (EMAIL, AS_EMAIL),
];

static RULES: LazyLock<Option<Vec<(Regex, &'static str)>>> = LazyLock::new(|| {
    let mut rules = Vec::with_capacity(VOCABULARY.len());
    for (pattern, replacement) in VOCABULARY {
        rules.push((Regex::new(pattern).ok()?, replacement));
    }
    Some(rules)
});

pub(crate) fn scrub(input: &str) -> Cow<'_, str> {
    let Some(rules) = RULES.as_ref() else {
        return if input.ends_with('\n') {
            Cow::Owned(format!("{UNAVAILABLE}\n"))
        } else {
            Cow::Borrowed(UNAVAILABLE)
        };
    };

    let mut out = Cow::Borrowed(input);
    for (pattern, replacement) in rules {
        let replaced = match pattern.replace_all(out.as_ref(), *replacement) {
            Cow::Owned(owned) => Some(owned),
            Cow::Borrowed(_) => None,
        };
        if let Some(owned) = replaced {
            out = Cow::Owned(owned);
        }
    }
    out
}

pub(crate) fn scrubbed<M>(inner: M) -> ScrubbingMakeWriter<M> {
    ScrubbingMakeWriter { inner }
}

pub(crate) struct ScrubbingMakeWriter<M> {
    inner: M,
}

impl<'a, M> MakeWriter<'a> for ScrubbingMakeWriter<M>
where
    M: MakeWriter<'a>,
{
    type Writer = ScrubbingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        ScrubbingWriter::new(self.inner.make_writer())
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        ScrubbingWriter::new(self.inner.make_writer_for(meta))
    }
}

pub(crate) struct ScrubbingWriter<W: io::Write> {
    inner: W,
    pending: Vec<u8>,
}

impl<W: io::Write> ScrubbingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
        }
    }

    fn emit_complete_lines(&mut self) -> io::Result<()> {
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            self.emit(&line)?;
        }
        Ok(())
    }

    fn emit_pending(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let tail = std::mem::take(&mut self.pending);
        self.emit(&tail)
    }

    fn emit(&mut self, chunk: &[u8]) -> io::Result<()> {
        let text = String::from_utf8_lossy(chunk);
        self.inner.write_all(scrub(&text).as_bytes())
    }
}

impl<W: io::Write> io::Write for ScrubbingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        self.emit_complete_lines()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.emit_complete_lines()?;
        self.emit_pending()?;
        self.inner.flush()
    }
}

impl<W: io::Write> Drop for ScrubbingWriter<W> {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the log writer cannot report its own write failures"
    )]
    fn drop(&mut self) {
        let _ = self.emit_complete_lines();
        let _ = self.emit_pending();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::layer::SubscriberExt as _;

    use super::*;

    const ONE_SAMPLE_PER_RULE: [(&str, &str); VOCABULARY.len()] = [
        (
            "pool checkout log.target=\"sqlx::pool\" log.line=12 done",
            "pool checkout done",
        ),
        (
            "at /home/bob/.cargo/registry/src/index.crates.io-6f17d22bba15001f/hyper-1.6.0/src/proto/h1/conn.rs:122:9",
            "at <dependency>",
        ),
        (
            "refresh rejected eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJmb3JnZSJ9.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk end",
            "refresh rejected <redacted:token> end",
        ),
        (
            "sending Bearer ya29.a0AfH6SMBxQ3nO7pQ upstream",
            "sending Bearer <redacted:token> upstream",
        ),
        (
            "PASS oauth:9fa8sd7f6a5s4d3f2g1h",
            "PASS oauth:<redacted:token>",
        ),
        (
            "loaded ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8",
            "loaded <redacted:token>",
        ),
        (
            "POST https://id.twitch.tv/oauth2/token?client_id=abc&client_secret=shhhh done",
            "POST https://id.twitch.tv/oauth2/token?<redacted:query> done",
        ),
        (
            "client_secret=hunter2hunter2",
            "client_secret=<redacted:token>",
        ),
        ("watching /home/bob/Source/x", "watching ~/Source/x"),
        (
            "watching C:\\Users\\bob\\AppData\\Roaming\\forge",
            "watching ~\\AppData\\Roaming\\forge",
        ),
        (
            "mail to streamer.name+alerts@example.com failed",
            "mail to <redacted:email> failed",
        ),
    ];

    const VENDOR_KEY_SHAPES: [&str; 7] = [
        "sk-A1b2C3d4E5f6G7h8I9j0K1l2",
        "sk_a1b2c3d4e5f6g7h8i9j0k1l2m3n4",
        "xoxb-123456789012-abcdefghijkl",
        "AIzaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q",
        "gho_A1b2C3d4E5f6G7h8I9j0K1l2",
        "github_pat_11ABCDEFG0aBcDeFgHiJkL",
        "AKIAIOSFODNN7EXAMPLE",
    ];

    const DIAGNOSTICS_THAT_MUST_SURVIVE: [&str; 17] = [
        "digest e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "sha256=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "event 01ARZ3NDEKTSV4RRFFQ69G5FAV replayed",
        "token_count=128000",
        "level=info",
        "overlay socket ws://127.0.0.1:8080/ws/v1/overlay",
        "wrote forge.log.2026-09-06 (4 KiB)",
        "opening /tmp/forge-smoke/data.db",
        "hey chat, thanks for the raid @viewer",
        "connected target=twitch user_id=1234567 latency_ms=42",
        "panicked at crates/forge-runtime/src/engine.rs:42:7",
        "wrote /tmp/.cargo-note.txt",
        "catalog.file=overlay.css",
        "logger=hyper",
        "log.level=debug",
        "/rustc/abc123/library/core/src/panic.rs",
        "/rustc/nothexes/library/core/src/panic.rs",
    ];

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Sink {
        fn written(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Sink {
        type Writer = Sink;

        fn make_writer(&'a self) -> Sink {
            self.clone()
        }
    }

    #[test]
    fn every_rule_redacts_its_secret_with_a_placeholder_naming_the_class_it_removed() {
        for (raw, expected) in ONE_SAMPLE_PER_RULE {
            assert_eq!(scrub(raw), expected, "input {raw:?}");
        }
    }

    #[test]
    fn every_rule_in_the_vocabulary_is_the_one_its_sample_exercises() {
        for ((pattern, _), (raw, _)) in VOCABULARY.iter().zip(ONE_SAMPLE_PER_RULE) {
            let rule = Regex::new(pattern).unwrap();
            assert!(rule.is_match(raw), "{pattern:?} does not match {raw:?}");
        }
    }

    #[test]
    fn a_third_party_source_path_collapses_to_one_marker_wherever_it_sits() {
        for (raw, expected) in [
            (
                "panicked at /Users/amy/.cargo/git/checkouts/gpui-abc/src/app.rs:10:5: boom",
                "panicked at <dependency> boom",
            ),
            (
                "C:\\Users\\amy\\.rustup\\toolchains\\stable\\lib\\rustlib\\src\\panic.rs:1",
                "<dependency>",
            ),
            (
                "C:\\Users\\Jane Doe\\.cargo\\registry\\src\\a.rs:3",
                "~ <dependency>",
            ),
            (
                "/rustc/05f9846f893b09a1be1fc8560e33fc3c815cfecb/library/core/src/panic.rs:70",
                "<dependency>",
            ),
            (
                "file=\"/home/bob/.cargo/registry/src/a/b.rs\" line=3",
                "file=\"<dependency>\" line=3",
            ),
            ("src=/home/bob/.cargo/a.rs", "src=<dependency>"),
            ("(/home/bob/.cargo/a.rs)", "(<dependency>)"),
        ] {
            assert_eq!(scrub(raw), expected, "input {raw:?}");
        }
    }

    #[test]
    fn scrubbing_rewrites_home_paths_and_quoted_or_named_credentials() {
        for (label, input, expected) in [
            (
                "a_path_in_the_users_own_checkout_keeps_everything_after_the_home_directory",
                "panicked at /home/bob/Source/forge/crates/app/src/main.rs:1:2",
                "panicked at ~/Source/forge/crates/app/src/main.rs:1:2",
            ),
            (
                "a_quoted_credential_value_is_redacted_together_with_its_quotes",
                "{\"refresh_token\": \"abcdef123456\"}",
                "{\"refresh_token\": <redacted:token>}",
            ),
        ] {
            assert_eq!(scrub(input), expected, "{label}");
        }
    }

    #[test]
    fn every_field_of_the_log_bridge_leaves_no_trace_or_stray_space() {
        for (raw, expected) in [
            ("a log.file=\"C:\\a b\\c.rs\" b", "a b"),
            ("a log.file=src/lib.rs b", "a b"),
            ("a log.line=12 b", "a b"),
            ("a log.module_path=hyper::client b", "a b"),
            ("a log.target=hyper b", "a b"),
            ("a\tlog.target=hyper", "a"),
            ("a log.target=hyper\nb log.line=1\n", "a\nb\n"),
        ] {
            assert_eq!(scrub(raw), expected, "input {raw:?}");
        }
    }

    #[test]
    fn every_vendor_key_shape_in_the_vocabulary_is_redacted() {
        for key in VENDOR_KEY_SHAPES {
            assert_eq!(
                scrub(&format!("loaded {key} from env")),
                "loaded <redacted:token> from env",
                "vendor key {key:?}",
            );
        }
    }

    #[test]
    fn ordinary_diagnostics_pass_through_untouched() {
        for line in DIAGNOSTICS_THAT_MUST_SURVIVE {
            assert_eq!(scrub(line), line, "diagnostic must survive scrubbing");
        }
    }

    #[test]
    fn a_credential_name_before_a_separator_redacts_even_a_harmless_value() {
        assert_eq!(scrub("token: expired"), "token: <redacted:token>");
    }

    #[test]
    fn scrubbing_already_scrubbed_output_changes_nothing() {
        let corpus = ONE_SAMPLE_PER_RULE
            .iter()
            .map(|(raw, _)| (*raw).to_owned())
            .chain(VENDOR_KEY_SHAPES.iter().map(|key| format!("loaded {key}")))
            .chain(
                DIAGNOSTICS_THAT_MUST_SURVIVE
                    .iter()
                    .map(|line| (*line).to_owned()),
            );

        for raw in corpus {
            let once = scrub(&raw).into_owned();
            assert_eq!(scrub(&once), once, "input {raw:?}");
        }
    }

    #[test]
    fn a_secret_split_across_two_writes_is_held_back_until_the_line_can_be_scrubbed() {
        let sink = Sink::default();
        let mut writer = ScrubbingWriter::new(sink.clone());

        writer.write_all(b"login token=abcd").unwrap();
        assert_eq!(
            sink.written(),
            "",
            "a half-written line must not escape before the scrubber can see it whole",
        );

        writer.write_all(b"ef123456 ok\n").unwrap();
        assert_eq!(sink.written(), "login token=<redacted:token> ok\n");
    }

    #[test]
    fn every_line_of_a_multi_line_write_is_scrubbed_and_the_remainder_stays_buffered() {
        let sink = Sink::default();
        let mut writer = ScrubbingWriter::new(sink.clone());

        writer
            .write_all(b"a token=aaaaaaaa\nb password=bbbbbbbb\ntrailing")
            .unwrap();

        assert_eq!(
            sink.written(),
            "a token=<redacted:token>\nb password=<redacted:token>\n",
        );
    }

    #[test]
    fn flush_emits_a_trailing_line_that_never_got_a_newline() {
        let sink = Sink::default();
        let mut writer = ScrubbingWriter::new(sink.clone());

        writer.write_all(b"tail token=abcdef123456").unwrap();
        writer.flush().unwrap();

        assert_eq!(sink.written(), "tail token=<redacted:token>");
    }

    #[test]
    fn dropping_the_writer_without_flushing_still_emits_the_buffered_tail() {
        let sink = Sink::default();
        let mut writer = ScrubbingWriter::new(sink.clone());

        writer.write_all(b"tail token=abcdef123456").unwrap();
        drop(writer);

        assert_eq!(sink.written(), "tail token=<redacted:token>");
    }

    #[test]
    fn a_secret_field_never_reaches_the_sink_whether_the_layer_colours_its_output_or_not() {
        for ansi in [false, true] {
            let sink = Sink::default();
            let subscriber = tracing_subscriber::registry().with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(ansi)
                    .with_writer(scrubbed(sink.clone())),
            );
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(token = "abcdef123456", "auth refreshed");
            });

            assert!(
                !sink.written().contains("abcdef123456"),
                "secret survived with_ansi({ansi}): {:?}",
                sink.written(),
            );
        }
    }
}
