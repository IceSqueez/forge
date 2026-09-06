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
