use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::crash_record::CrashRecord;

pub const ISSUE_URL_MAX_BYTES: usize = 8 * 1024;
pub const CRASH_TEMPLATE: &str = "crash.yml";
pub const BUG_TEMPLATE: &str = "bug_report.yml";
pub const FIELD_VERSION: &str = "version";
pub const FIELD_CRASH: &str = "crash";
pub const FIELD_SYSTEM_INFO: &str = "system-info";
pub const FIELD_BACKTRACE: &str = "backtrace";
pub const CRASH_REPORT_FILE_NAME: &str = "crash.txt";

const CRASH_TITLE_PREFIX: &str = "Crash: ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueField {
    pub id: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueForm {
    pub template: &'static str,
    pub title: Option<String>,
    pub fields: Vec<IssueField>,
}

impl IssueForm {
    pub fn crash(record: &CrashRecord, system_info: &str) -> Self {
        Self {
            template: CRASH_TEMPLATE,
            title: Some(format!("{CRASH_TITLE_PREFIX}{}", record.summary())),
            fields: vec![
                IssueField {
                    id: FIELD_VERSION,
                    value: record.version_line(),
                },
                IssueField {
                    id: FIELD_CRASH,
                    value: record.details_text(),
                },
                IssueField {
                    id: FIELD_SYSTEM_INFO,
                    value: system_info.to_owned(),
                },
                IssueField {
                    id: FIELD_BACKTRACE,
                    value: record.backtrace.clone(),
                },
            ],
        }
    }

    pub fn problem(version_line: String, system_info: &str) -> Self {
        Self {
            template: BUG_TEMPLATE,
            title: None,
            fields: vec![
                IssueField {
                    id: FIELD_VERSION,
                    value: version_line,
                },
                IssueField {
                    id: FIELD_SYSTEM_INFO,
                    value: system_info.to_owned(),
                },
            ],
        }
    }

    pub fn url(&self, repo_url: &str) -> String {
        let mut url = format!(
            "{}/issues/new?template={}",
            repo_url.trim_end_matches('/'),
            encode(self.template)
        );
        if let Some(title) = &self.title {
            url.push_str("&title=");
            url.push_str(&encode(title));
        }
        for field in &self.fields {
            url.push('&');
            url.push_str(&encode(field.id));
            url.push('=');
            url.push_str(&encode(&field.value));
        }
        url
    }

    pub fn fitted(self, repo_url: &str, max_bytes: usize) -> Self {
        if self.url(repo_url).len() <= max_bytes {
            return self;
        }
        let Some(index) = self
            .fields
            .iter()
            .position(|field| field.id == FIELD_BACKTRACE)
        else {
            return self;
        };
        let lines: Vec<String> = self.fields[index]
            .value
            .lines()
            .map(str::to_owned)
            .collect();
        let with_lines = |kept: usize| {
            let mut form = self.clone();
            form.fields[index].value = truncated_backtrace(&lines[..kept]);
            form
        };
        let (mut fits, mut overflows) = (0, lines.len());
        while overflows - fits > 1 {
            let middle = fits + (overflows - fits) / 2;
            if with_lines(middle).url(repo_url).len() <= max_bytes {
                fits = middle;
            } else {
                overflows = middle;
            }
        }
        with_lines(fits)
    }

    pub fn preview_text(&self) -> String {
        let mut sections = Vec::with_capacity(self.fields.len() + 1);
        if let Some(title) = &self.title {
            sections.push(format!("title:\n{title}"));
        }
        sections.extend(
            self.fields
                .iter()
                .map(|field| format!("{}:\n{}", field.id, field.value)),
        );
        sections.join("\n\n")
    }
}

fn truncated_backtrace(kept: &[String]) -> String {
    let mut text = kept.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&backtrace_truncated_marker());
    text
}

pub fn backtrace_truncated_marker() -> String {
    format!(
        "... [backtrace truncated - the full text is in {CRASH_REPORT_FILE_NAME} in the saved report folder]"
    )
}

pub fn encode(text: &str) -> String {
    utf8_percent_encode(text, NON_ALPHANUMERIC).to_string()
}
