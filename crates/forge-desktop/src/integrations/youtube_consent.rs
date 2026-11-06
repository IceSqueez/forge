use std::ops::Range;

use forge_components::{ActivePresentation, BORDER_THIN, FONT_XS, ForgePalette, tr};
use forge_platform_youtube::{FORGE_PRIVACY_POLICY_URL, YOUTUBE_TERMS_OF_SERVICE_URL};
use gpui::{
    App, ElementId, HighlightStyle, Hsla, InteractiveText, StyledText, UnderlineStyle, Window, div,
    prelude::*,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyLink {
    YouTubeTerms,
    ForgePrivacy,
}

impl PolicyLink {
    const IN_TEXT_ORDER: [PolicyLink; 2] = [PolicyLink::YouTubeTerms, PolicyLink::ForgePrivacy];

    pub(crate) fn url(self) -> &'static str {
        match self {
            PolicyLink::YouTubeTerms => YOUTUBE_TERMS_OF_SERVICE_URL,
            PolicyLink::ForgePrivacy => FORGE_PRIVACY_POLICY_URL,
        }
    }
}

pub(crate) struct ConsentText {
    pub(crate) text: String,
    pub(crate) link_ranges: [Range<usize>; 2],
}

pub(crate) fn consent_text(
    prefix: &str,
    terms: &str,
    joiner: &str,
    privacy: &str,
    suffix: &str,
) -> ConsentText {
    let terms_start = prefix.len();
    let terms_end = terms_start + terms.len();
    let privacy_start = terms_end + joiner.len();
    let privacy_end = privacy_start + privacy.len();
    ConsentText {
        text: format!("{prefix}{terms}{joiner}{privacy}{suffix}"),
        link_ranges: [terms_start..terms_end, privacy_start..privacy_end],
    }
}

pub(crate) fn youtube_consent_line(
    id: impl Into<ElementId>,
    palette: &ForgePalette,
    cx: &App,
    on_open: impl Fn(&PolicyLink, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let consent = consent_text(
        &tr!("youtube_consent_prefix"),
        &tr!("youtube_consent_terms_link"),
        &tr!("youtube_consent_joiner"),
        &tr!("youtube_consent_privacy_link"),
        &tr!("youtube_consent_suffix"),
    );
    let link_color = Hsla::from(palette.info);
    let link_style = HighlightStyle {
        color: Some(link_color),
        underline: Some(UnderlineStyle {
            thickness: BORDER_THIN,
            color: Some(link_color),
            wavy: false,
        }),
        ..HighlightStyle::default()
    };
    let highlights: Vec<_> = consent
        .link_ranges
        .iter()
        .map(|range| (range.clone(), link_style))
        .collect();
    let styled = StyledText::new(consent.text).with_highlights(highlights);
    let text = InteractiveText::new(id, styled).on_click(
        consent.link_ranges.to_vec(),
        move |index, window, cx| {
            if let Some(link) = PolicyLink::IN_TEXT_ORDER.get(index) {
                on_open(link, window, cx);
            }
        },
    );
    div()
        .w_full()
        .font_family(cx.body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(text)
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;

    use super::*;
    use crate::app::i18n::message_in;

    fn consent_in(language: Language) -> ConsentText {
        consent_text(
            &message_in(language, "youtube_consent_prefix"),
            &message_in(language, "youtube_consent_terms_link"),
            &message_in(language, "youtube_consent_joiner"),
            &message_in(language, "youtube_consent_privacy_link"),
            &message_in(language, "youtube_consent_suffix"),
        )
    }

    #[test]
    fn consent_sentence_reads_whole_with_single_spaces_around_each_link() {
        for (language, expected) in [
            (
                Language::En,
                "By connecting YouTube you agree to the YouTube Terms of Service and the forge Privacy Policy.",
            ),
            (
                Language::Uk,
                "Підключаючи YouTube, ти погоджуєшся з Умовами використання YouTube і Політикою конфіденційності forge.",
            ),
        ] {
            assert_eq!(consent_in(language).text, expected, "{language:?}");
        }
    }

    #[test]
    fn link_ranges_slice_exactly_the_link_labels_in_each_language() {
        for (language, terms, privacy) in [
            (
                Language::En,
                "YouTube Terms of Service",
                "forge Privacy Policy",
            ),
            (
                Language::Uk,
                "Умовами використання YouTube",
                "Політикою конфіденційності forge",
            ),
        ] {
            let consent = consent_in(language);
            let [terms_range, privacy_range] = consent.link_ranges.clone();
            assert_eq!(&consent.text[terms_range], terms, "{language:?}");
            assert_eq!(&consent.text[privacy_range], privacy, "{language:?}");
        }
    }

    #[test]
    fn clicked_range_index_opens_the_link_shown_at_that_position() {
        let consent = consent_in(Language::En);
        for (index, label, url) in [
            (
                0,
                "YouTube Terms of Service",
                "https://www.youtube.com/t/terms",
            ),
            (
                1,
                "forge Privacy Policy",
                "https://icesqueez.com/en/forge/privacy/",
            ),
        ] {
            assert_eq!(
                &consent.text[consent.link_ranges[index].clone()],
                label,
                "{index}"
            );
            assert_eq!(PolicyLink::IN_TEXT_ORDER[index].url(), url, "{index}");
        }
    }
}
