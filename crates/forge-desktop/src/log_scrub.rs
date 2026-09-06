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

const UNAVAILABLE: &str = "<redacted:log scrubber unavailable>";

const VOCABULARY: [(&str, &str); 9] = [
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

/// Backstop only: matches credential, URL-query, home-path and email shapes; free-form prose has no signature it can see.
pub fn scrub(input: &str) -> Cow<'_, str> {
    let Some(rules) = RULES.as_ref() else {
        return match input.ends_with('\n') {
            true => Cow::Owned(format!("{UNAVAILABLE}\n")),
            false => Cow::Borrowed(UNAVAILABLE),
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

pub fn scrubbed<M>(inner: M) -> ScrubbingMakeWriter<M> {
    ScrubbingMakeWriter { inner }
}

pub struct ScrubbingMakeWriter<M> {
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

pub struct ScrubbingWriter<W: io::Write> {
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
    fn drop(&mut self) {
        let _ = self.emit_complete_lines();
        let _ = self.emit_pending();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::io::Write as _;
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::layer::SubscriberExt as _;

    use super::*;

    const ONE_SAMPLE_PER_RULE: [(&str, &str); 9] = [
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

    const DIAGNOSTICS_THAT_MUST_SURVIVE: [&str; 10] = [
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
    fn a_quoted_credential_value_is_redacted_together_with_its_quotes() {
        assert_eq!(
            scrub("{\"refresh_token\": \"abcdef123456\"}"),
            "{\"refresh_token\": <redacted:token>}",
        );
    }

    #[test]
    fn ordinary_diagnostics_pass_through_untouched() {
        for line in DIAGNOSTICS_THAT_MUST_SURVIVE {
            assert_eq!(scrub(line), line, "diagnostic must survive scrubbing");
        }
    }

    #[test]
    fn a_credential_name_before_a_separator_redacts_even_a_harmless_value() {
        // Why: the backstop cannot tell a token from prose, and invariant #7 prefers
        // losing a diagnostic word over shipping a token to a public issue tracker.
        // Pinned so the over-scrub is never "fixed" into a leak.
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
        // Why: the console layer keeps ANSI on while the file layer turns it off, so the
        // vocabulary has to survive the SGR codes tracing wraps around a field name.
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
