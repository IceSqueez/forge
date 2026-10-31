use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::crash_record::CrashRecord;

pub(crate) const ISSUE_URL_MAX_BYTES: usize = 8 * 1024;
pub(crate) const CRASH_TEMPLATE: &str = "crash.yml";
pub(crate) const BUG_TEMPLATE: &str = "bug_report.yml";
pub(crate) const FIELD_VERSION: &str = "version";
pub(crate) const FIELD_CRASH: &str = "crash";
pub(crate) const FIELD_SYSTEM_INFO: &str = "system-info";
pub(crate) const FIELD_BACKTRACE: &str = "backtrace";
pub(crate) const CRASH_REPORT_FILE_NAME: &str = "crash.txt";
pub(crate) const SYSTEM_INFO_FILE_NAME: &str = "system-info.txt";

const CRASH_TITLE_PREFIX: &str = "Crash: ";
const TRIM_ORDER: [(&str, &str); 2] = [
    (FIELD_SYSTEM_INFO, SYSTEM_INFO_FILE_NAME),
    (FIELD_CRASH, CRASH_REPORT_FILE_NAME),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssueField {
    pub id: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssueForm {
    pub template: &'static str,
    pub title: Option<String>,
    pub fields: Vec<IssueField>,
}

impl IssueForm {
    pub(crate) fn crash(record: &CrashRecord, system_info: &str) -> Self {
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

    pub(crate) fn problem(version_line: String, system_info: &str) -> Self {
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

    pub(crate) fn url(&self, repo_url: &str) -> String {
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

    pub(crate) fn fitted(self, repo_url: &str, max_bytes: usize) -> Self {
        let mut form = self.with_fitted_backtrace(repo_url, max_bytes);
        for (id, full_text_file) in TRIM_ORDER {
            let url_len = form.url(repo_url).len();
            if url_len <= max_bytes {
                break;
            }
            if let Some(field) = form.fields.iter_mut().find(|field| field.id == id) {
                trim_to_free(&mut field.value, url_len - max_bytes, full_text_file);
            }
        }
        form
    }

    fn with_fitted_backtrace(self, repo_url: &str, max_bytes: usize) -> Self {
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

    pub(crate) fn preview_text(&self) -> String {
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

fn trim_to_free(value: &mut String, excess_encoded_bytes: usize, full_text_file: &str) {
    let marker = field_truncated_marker(full_text_file);
    let needed = excess_encoded_bytes + encode(&marker).len();
    let mut freed = 0;
    let mut cut = value.len();
    for (index, ch) in value.char_indices().rev() {
        if freed >= needed {
            break;
        }
        freed += encode(ch.encode_utf8(&mut [0; 4])).len();
        cut = index;
    }
    value.truncate(cut);
    value.push_str(&marker);
}

pub(crate) fn field_truncated_marker(full_text_file: &str) -> String {
    format!("... [truncated - the full text is in {full_text_file} in the saved report folder]")
}

pub(crate) fn backtrace_truncated_marker() -> String {
    format!(
        "... [backtrace truncated - the full text is in {CRASH_REPORT_FILE_NAME} in the saved report folder]"
    )
}

pub(crate) fn encode(text: &str) -> String {
    utf8_percent_encode(text, NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use percent_encoding::percent_decode_str;

    use super::{
        CRASH_REPORT_FILE_NAME, FIELD_BACKTRACE, FIELD_CRASH, FIELD_SYSTEM_INFO, FIELD_VERSION,
        ISSUE_URL_MAX_BYTES, IssueForm, SYSTEM_INFO_FILE_NAME, backtrace_truncated_marker, encode,
        field_truncated_marker,
    };
    use crate::crash_record::CrashRecord;

    const REPO: &str = "https://github.com/owner/forge";

    fn crash_record(backtrace: String) -> CrashRecord {
        CrashRecord {
            message: "boom\nsecond line".to_owned(),
            location: Some("crates/forge-desktop/src/root.rs:42:9".to_owned()),
            thread: Some("main".to_owned()),
            backtrace,
            forge_version: "0.6.2".to_owned(),
            build_commit: "abc1234".to_owned(),
            occurred_at_utc: "2026-10-16T10:00:00Z".to_owned(),
            process_id: 7,
            system_info: None,
        }
    }

    fn long_backtrace() -> String {
        (0..2000)
            .map(|frame| format!("{frame:>4}: forge::module_{frame}::function_{frame}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn query(url: &str) -> Vec<(String, String)> {
        let (_, query) = url.split_once('?').unwrap();
        query
            .split('&')
            .map(|pair| {
                let (key, value) = pair.split_once('=').unwrap();
                let decode =
                    |part: &str| percent_decode_str(part).decode_utf8().unwrap().into_owned();
                (decode(key), decode(value))
            })
            .collect()
    }

    fn value<'a>(form: &'a IssueForm, id: &str) -> &'a str {
        &form
            .fields
            .iter()
            .find(|field| field.id == id)
            .unwrap()
            .value
    }

    fn template_field_ids(template: &str) -> Vec<String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.github/ISSUE_TEMPLATE")
            .join(template);
        std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter_map(|line| line.trim().strip_prefix("id: "))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_prefilled_field_id_exists_in_its_issue_template() {
        let forms = [
            IssueForm::crash(&crash_record("  0: main".to_owned()), "OS: Linux"),
            IssueForm::problem("v0.6.2 (abc1234)".to_owned(), "OS: Linux"),
        ];
        for form in forms {
            let ids = template_field_ids(form.template);
            for field in &form.fields {
                assert!(
                    ids.iter().any(|id| id == field.id),
                    "{} has no field with id {}",
                    form.template,
                    field.id
                );
            }
        }
    }

    #[test]
    fn url_round_trips_reserved_characters_newlines_and_unicode() {
        let tricky = "a&b=c#d+e%f?g/h\nline two\r\nУкраїна 🎙";
        let url = IssueForm::problem("v0.6.2 (abc1234)".to_owned(), tricky).url(REPO);
        assert_eq!(
            query(&url),
            vec![
                ("template".to_owned(), "bug_report.yml".to_owned()),
                ("version".to_owned(), "v0.6.2 (abc1234)".to_owned()),
                ("system-info".to_owned(), tricky.to_owned()),
            ]
        );
    }

    #[test]
    fn url_targets_the_new_issue_page_even_with_a_trailing_slash() {
        let url = IssueForm::problem("v1".to_owned(), "").url(&format!("{REPO}/"));
        assert!(url.starts_with(&format!("{REPO}/issues/new?")), "{url}");
    }

    #[test]
    fn crash_url_carries_the_summary_as_title() {
        let form = IssueForm::crash(&crash_record("  0: main".to_owned()), "OS: Linux");
        let pairs = query(&form.url(REPO));
        assert!(pairs.contains(&("title".to_owned(), "Crash: boom".to_owned())));
    }

    #[test]
    fn problem_url_leaves_the_title_to_the_template() {
        let pairs = query(&IssueForm::problem("v1".to_owned(), "").url(REPO));
        assert!(pairs.iter().all(|(key, _)| key != "title"));
    }

    #[test]
    fn fitted_leaves_a_form_under_the_cap_untouched() {
        let form = IssueForm::crash(&crash_record("  0: main".to_owned()), "OS: Linux");
        assert_eq!(form.clone().fitted(REPO, ISSUE_URL_MAX_BYTES), form);
    }

    #[test]
    fn fitted_brings_an_oversized_crash_url_under_the_cap() {
        let form = IssueForm::crash(&crash_record(long_backtrace()), "OS: Linux");
        assert!(form.url(REPO).len() > ISSUE_URL_MAX_BYTES);
        let fitted = form.fitted(REPO, ISSUE_URL_MAX_BYTES);
        assert!(fitted.url(REPO).len() <= ISSUE_URL_MAX_BYTES);
    }

    #[test]
    fn fitted_keeps_whole_leading_backtrace_lines_then_the_marker() {
        let backtrace = long_backtrace();
        let form = IssueForm::crash(&crash_record(backtrace.clone()), "OS: Linux");
        let fitted = form.fitted(REPO, ISSUE_URL_MAX_BYTES);
        let trimmed = value(&fitted, FIELD_BACKTRACE);
        let kept = trimmed
            .strip_suffix(&backtrace_truncated_marker())
            .unwrap()
            .trim_end_matches('\n');
        assert!(!kept.is_empty());
        assert!(backtrace.starts_with(&format!("{kept}\n")), "{kept}");
    }

    #[test]
    fn fitted_keeps_as_many_backtrace_lines_as_the_cap_allows() {
        let backtrace = long_backtrace();
        let form = IssueForm::crash(&crash_record(backtrace.clone()), "OS: Linux");
        let fitted = form.fitted(REPO, ISSUE_URL_MAX_BYTES);
        let kept_lines = value(&fitted, FIELD_BACKTRACE).lines().count() - 1;
        let next_line = backtrace.lines().nth(kept_lines).unwrap();
        let grown = fitted.url(REPO).len() + encode(&format!("{next_line}\n")).len();
        assert!(grown > ISSUE_URL_MAX_BYTES);
    }

    #[test]
    fn fitted_trims_nothing_but_the_backtrace() {
        let form = IssueForm::crash(&crash_record(long_backtrace()), "OS: Linux");
        let fitted = form.clone().fitted(REPO, ISSUE_URL_MAX_BYTES);
        let without_backtrace = |form: &IssueForm| {
            let mut form = form.clone();
            form.fields.retain(|field| field.id != FIELD_BACKTRACE);
            form
        };
        assert_eq!(without_backtrace(&fitted), without_backtrace(&form));
    }

    fn emoji_cyrillic_message() -> String {
        "\u{1F399}\u{439}".repeat(500)
    }

    fn oversized_crash_form(system_info: &str) -> IssueForm {
        let record = CrashRecord {
            message: emoji_cyrillic_message(),
            ..crash_record(backtrace_truncated_marker())
        };
        IssueForm::crash(&record, system_info)
    }

    fn assert_header_unchanged(fitted: &IssueForm, original: &IssueForm) {
        assert_eq!(fitted.template, original.template);
        assert_eq!(fitted.title, original.title);
        assert_eq!(value(fitted, FIELD_VERSION), value(original, FIELD_VERSION));
    }

    fn assert_url_is_valid_utf8_within_cap(form: &IssueForm) {
        let url = form.url(REPO);
        assert!(url.len() <= ISSUE_URL_MAX_BYTES, "{}", url.len());
        let pairs = query(&url);
        assert_eq!(
            pairs.len(),
            form.fields.len() + 1 + usize::from(form.title.is_some())
        );
    }

    #[test]
    fn fitted_trims_a_long_emoji_cyrillic_crash_message_with_the_crash_marker() {
        let form = oversized_crash_form("OS: Linux");
        assert!(form.url(REPO).len() > ISSUE_URL_MAX_BYTES);
        let fitted = form.clone().fitted(REPO, ISSUE_URL_MAX_BYTES);
        assert_url_is_valid_utf8_within_cap(&fitted);
        assert!(
            value(&fitted, FIELD_CRASH).ends_with(&field_truncated_marker(CRASH_REPORT_FILE_NAME))
        );
        assert_header_unchanged(&fitted, &form);
    }

    #[test]
    fn fitted_trims_only_system_info_when_the_crash_text_already_fits() {
        let form = IssueForm::crash(
            &crash_record(backtrace_truncated_marker()),
            &"\u{e9}".repeat(5000),
        );
        let fitted = form.clone().fitted(REPO, ISSUE_URL_MAX_BYTES);
        assert_url_is_valid_utf8_within_cap(&fitted);
        let system_info = value(&fitted, FIELD_SYSTEM_INFO);
        let kept = system_info
            .strip_suffix(&field_truncated_marker(SYSTEM_INFO_FILE_NAME))
            .unwrap();
        assert!(!kept.is_empty());
        assert!(value(&form, FIELD_SYSTEM_INFO).starts_with(kept));
        assert_eq!(value(&fitted, FIELD_CRASH), value(&form, FIELD_CRASH));
        assert_eq!(
            value(&fitted, FIELD_BACKTRACE),
            backtrace_truncated_marker()
        );
        assert_header_unchanged(&fitted, &form);
    }

    #[test]
    fn fitted_trims_the_crash_after_system_info_alone_cannot_free_enough() {
        let form = oversized_crash_form(&"\u{e9}".repeat(5000));
        let fitted = form.clone().fitted(REPO, ISSUE_URL_MAX_BYTES);
        assert_url_is_valid_utf8_within_cap(&fitted);
        assert_eq!(
            value(&fitted, FIELD_SYSTEM_INFO),
            field_truncated_marker(SYSTEM_INFO_FILE_NAME)
        );
        assert!(
            value(&fitted, FIELD_CRASH).ends_with(&field_truncated_marker(CRASH_REPORT_FILE_NAME))
        );
        assert_header_unchanged(&fitted, &form);
    }

    #[test]
    fn fitted_problem_form_names_system_info_in_its_marker_never_crash() {
        let form = IssueForm::problem("v0.6.2 (abc1234)".to_owned(), &"\u{e9}".repeat(5000));
        let fitted = form.clone().fitted(REPO, ISSUE_URL_MAX_BYTES);
        assert_url_is_valid_utf8_within_cap(&fitted);
        let system_info = value(&fitted, FIELD_SYSTEM_INFO);
        assert!(system_info.ends_with(&field_truncated_marker(SYSTEM_INFO_FILE_NAME)));
        assert!(!system_info.contains(CRASH_REPORT_FILE_NAME));
        assert_header_unchanged(&fitted, &form);
    }

    #[test]
    fn fitted_cuts_four_byte_system_info_on_character_boundaries() {
        let original = "\u{1F399}".repeat(2000);
        let form = IssueForm::problem("v1".to_owned(), &original);
        let fitted = form.fitted(REPO, ISSUE_URL_MAX_BYTES);
        assert_url_is_valid_utf8_within_cap(&fitted);
        let kept = value(&fitted, FIELD_SYSTEM_INFO)
            .strip_suffix(&field_truncated_marker(SYSTEM_INFO_FILE_NAME))
            .unwrap();
        assert!(!kept.is_empty());
        assert!(original.starts_with(kept));
    }

    #[test]
    fn fitted_leaves_only_the_marker_when_the_cap_consumes_the_whole_field() {
        let tight = 10;
        let problem = IssueForm::problem("v1".to_owned(), &"x".repeat(50)).fitted(REPO, tight);
        assert_eq!(
            value(&problem, FIELD_SYSTEM_INFO),
            field_truncated_marker(SYSTEM_INFO_FILE_NAME)
        );
        let crash = oversized_crash_form(&"x".repeat(50)).fitted(REPO, tight);
        assert_eq!(
            value(&crash, FIELD_SYSTEM_INFO),
            field_truncated_marker(SYSTEM_INFO_FILE_NAME)
        );
        assert_eq!(
            value(&crash, FIELD_CRASH),
            field_truncated_marker(CRASH_REPORT_FILE_NAME)
        );
    }

    #[test]
    fn preview_lists_every_field_in_form_order_without_a_title() {
        let form = IssueForm::problem("v1".to_owned(), "OS: Linux");
        assert_eq!(
            form.preview_text(),
            "version:\nv1\n\nsystem-info:\nOS: Linux"
        );
    }

    #[test]
    fn preview_puts_the_crash_title_first_and_the_backtrace_last() {
        let preview =
            IssueForm::crash(&crash_record("  0: main".to_owned()), "OS: Linux").preview_text();
        assert!(
            preview.starts_with("title:\nCrash: boom\n\nversion:\n"),
            "{preview}"
        );
        assert!(preview.ends_with("\n\nbacktrace:\n  0: main"), "{preview}");
    }
}
