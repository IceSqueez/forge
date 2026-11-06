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
