#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_update::{
    GithubRepo, ManifestSource, PublishedRelease, UpdateError, parse_latest_release,
};

const WEB_URL_SPELLINGS: [&str; 5] = [
    "https://github.com/nova/forge",
    "https://github.com/nova/forge.git",
    "https://github.com/nova/forge/",
    "https://github.com/nova/forge.git/",
    "  https://github.com/nova/forge  ",
];

const NOT_GITHUB_WEB_URLS: [&str; 6] = [
    "https://gitlab.com/nova/forge",
    "http://github.com/nova/forge",
    "git@github.com:nova/forge.git",
    "https://github.com/",
    "https://github.com//",
    "",
];

#[test]
fn github_repo_reaches_the_release_api_whatever_the_web_url_spelling() {
    for spelling in WEB_URL_SPELLINGS {
        let repo = GithubRepo::parse(spelling).expect(spelling);
        assert_eq!(
            repo.latest_release_api_url(),
            "https://api.github.com/repos/nova/forge/releases/latest",
            "for {spelling:?}",
        );
    }
}

#[test]
fn both_manifest_entry_points_download_the_latest_signed_manifest_whatever_the_spelling() {
    let expected = ManifestSource::new(
        "https://github.com/nova/forge/releases/latest/download/forge-update.json".to_owned(),
        "https://github.com/nova/forge/releases/latest/download/forge-update.json.minisig"
            .to_owned(),
    );
    for spelling in WEB_URL_SPELLINGS {
        let from_repo = GithubRepo::parse(spelling).map(|repo| repo.latest_manifest_source());
        assert_eq!(from_repo.as_ref(), Some(&expected), "repo for {spelling:?}");
        assert_eq!(
            ManifestSource::github_latest(spelling).as_ref(),
            Some(&expected),
            "github_latest for {spelling:?}",
        );
    }
}

#[test]
fn anything_but_a_github_web_url_yields_no_repo_and_no_manifest_source() {
    for url in NOT_GITHUB_WEB_URLS {
        assert_eq!(GithubRepo::parse(url), None, "repo accepted {url:?}");
        assert_eq!(
            ManifestSource::github_latest(url),
            None,
            "manifest source accepted {url:?}",
        );
    }
}

#[test]
fn release_page_url_opens_the_v_tag_page_whatever_the_version_and_web_url_spelling() {
    for (repo, version, expected) in [
        (
            "https://github.com/nova/forge",
            "0.7.1",
            "https://github.com/nova/forge/releases/tag/v0.7.1",
        ),
        (
            "https://github.com/nova/forge",
            "v0.7.1",
            "https://github.com/nova/forge/releases/tag/v0.7.1",
        ),
        (
            "https://github.com/nova/forge.git",
            " v0.7.1 ",
            "https://github.com/nova/forge/releases/tag/v0.7.1",
        ),
        (
            "https://github.com/nova/forge.git/",
            "0.8.0-rc.1",
            "https://github.com/nova/forge/releases/tag/v0.8.0-rc.1",
        ),
    ] {
        let repo = GithubRepo::parse(repo).expect(repo);
        assert_eq!(
            repo.release_page_url(version).as_deref(),
            Some(expected),
            "for {version:?}",
        );
    }
}

#[test]
fn release_page_url_is_none_without_a_version() {
    let repo = GithubRepo::parse("https://github.com/nova/forge").unwrap();
    for version in ["", "v", "   ", " v "] {
        assert_eq!(repo.release_page_url(version), None, "accepted {version:?}");
    }
}

#[test]
fn parse_latest_release_takes_the_trimmed_tag_and_page_of_a_published_release() {
    for body in [
        r#"{"tag_name":" v0.6.0 ","html_url":" https://example.invalid/r ","draft":false}"#,
        r#"{"tag_name":"v0.6.0","html_url":"https://example.invalid/r"}"#,
        r#"{"id":7,"tag_name":"v0.6.0","html_url":"https://example.invalid/r","prerelease":false,"assets":[]}"#,
    ] {
        assert_eq!(
            parse_latest_release(body.as_bytes()).unwrap(),
            Some(PublishedRelease {
                tag: "v0.6.0".to_owned(),
                page_url: "https://example.invalid/r".to_owned(),
            }),
            "for {body:?}",
        );
    }
}

#[test]
fn parse_latest_release_skips_a_draft_even_with_an_empty_tag_or_page() {
    for body in [
        r#"{"tag_name":"v0.6.0","html_url":"https://example.invalid/r","draft":true}"#,
        r#"{"tag_name":"","html_url":"","draft":true}"#,
    ] {
        assert_eq!(
            parse_latest_release(body.as_bytes()).unwrap(),
            None,
            "for {body:?}",
        );
    }
}

#[test]
fn parse_latest_release_rejects_a_malformed_description() {
    for body in [
        r#"{"html_url":"https://example.invalid/r"}"#,
        r#"{"tag_name":"v0.6.0"}"#,
        r#"{"tag_name":"   ","html_url":"https://example.invalid/r"}"#,
        r#"{"tag_name":"v0.6.0","html_url":""}"#,
        r#"{"tag_name":42,"html_url":"https://example.invalid/r"}"#,
        r#"{"tag_name":"v0.6.0","html_url":"https://example.invalid/r","draft":"yes"}"#,
        r#"["v0.6.0"]"#,
        r#""v0.6.0""#,
        "null",
        "{not json",
        "",
    ] {
        let result = parse_latest_release(body.as_bytes());
        assert!(
            matches!(result, Err(UpdateError::MalformedRelease(_))),
            "{body:?} gave {result:?}",
        );
    }
}
