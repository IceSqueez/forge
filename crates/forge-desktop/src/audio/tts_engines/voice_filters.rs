use forge_components::{
    ActivePresentation, Density, FONT_LABEL, FONT_XXS, ForgePalette, InputEvent, SearchState,
    Spacing, choice_trigger, dropdown, dropdown_list, dropdown_row, segment, segmented, spacing,
    tr,
};
use gpui::{AnyElement, ClickEvent, Context, ElementId, FontWeight, Pixels, div, prelude::*, px};

use super::TtsEnginesView;
use crate::audio::voice_listing::{GenderFilter, VoiceFilter, VoiceListing, VoiceListingCache};

const LANGUAGE_SELECT_W: Pixels = px(200.0);

pub(super) struct VoiceBrowser {
    pub(super) search: SearchState,
    pub(super) filter: VoiceFilter,
    pub(super) language_menu_open: bool,
    pub(super) cache: VoiceListingCache,
}

impl VoiceBrowser {
    pub(super) fn new(cx: &mut Context<TtsEnginesView>) -> Self {
        Self {
            search: SearchState::new(cx, tr!("tts_engines_voices_search_placeholder")),
            filter: VoiceFilter::default(),
            language_menu_open: false,
            cache: VoiceListingCache::default(),
        }
    }
}

impl TtsEnginesView {
    pub(super) fn on_voice_search_event(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        if self.voices.search.on_changed(event) {
            self.voices.filter.folded_name = self.voices.search.query().to_owned();
            cx.notify();
        }
    }

    pub(super) fn reset_voice_filters(&mut self, cx: &mut Context<Self>) {
        self.voices.search.clear(cx);
        self.voices.filter = VoiceFilter::default();
        self.voices.language_menu_open = false;
        cx.notify();
    }

    fn toggle_language_menu(&mut self, cx: &mut Context<Self>) {
        self.voices.language_menu_open = !self.voices.language_menu_open;
        cx.notify();
    }

    fn close_language_menu(&mut self, cx: &mut Context<Self>) {
        self.voices.language_menu_open = false;
        cx.notify();
    }

    fn choose_voice_language(&mut self, language: Option<String>, cx: &mut Context<Self>) {
        self.voices.filter.language = language;
        self.voices.language_menu_open = false;
        cx.notify();
    }

    fn choose_voice_gender(&mut self, gender: GenderFilter, cx: &mut Context<Self>) {
        self.voices.filter.gender = gender;
        cx.notify();
    }

    pub(super) fn voice_filter_row(
        &self,
        listing: &VoiceListing,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current_gender = self.voices.filter.gender;
        let genders = GenderFilter::ALL
            .into_iter()
            .enumerate()
            .map(|(index, gender)| {
                segment(
                    ("tts-voice-gender", index),
                    gender.label(),
                    gender == current_gender,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.choose_voice_gender(gender, cx)
                    }),
                )
            })
            .collect();

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.voices.search.field().clone()),
            )
            .child(self.language_select(listing, palette, density, cx))
            .child(div().flex_shrink_0().child(segmented(genders, palette)))
            .into_any_element()
    }

    fn language_select(
        &self,
        listing: &VoiceListing,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = listing
            .selected_language_name()
            .map_or_else(|| tr!("tts_engines_voices_all_languages"), str::to_owned);
        let trigger = choice_trigger(
            "tts-voice-language",
            selected.into(),
            true,
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_language_menu(cx)),
            density,
            cx,
        );
        let menu = self
            .voices
            .language_menu_open
            .then(|| self.language_menu(listing, palette, cx));

        div()
            .relative()
            .w(LANGUAGE_SELECT_W)
            .flex_shrink_0()
            .child(trigger)
            .children(menu)
            .into_any_element()
    }

    fn language_menu(
        &self,
        listing: &VoiceListing,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let all = dropdown_row(
            "tts-voice-language-all",
            tr!("tts_engines_voices_all_languages"),
            palette,
        )
        .current(listing.selected_language.is_none())
        .suffix(Some(listing.engine_total.to_string().into()))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.choose_voice_language(None, cx)));

        let mut list = dropdown_list("tts-voice-language-list", palette).child(all);
        for (index, language) in listing.languages.iter().enumerate() {
            let key = language.key.clone();
            let current = listing.selected_language.as_ref() == Some(&language.key);
            list = list.child(
                dropdown_row(
                    ElementId::from(("tts-voice-language-row", index)),
                    language.name.clone(),
                    palette,
                )
                .current(current)
                .suffix(Some(language.count.to_string().into()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.choose_voice_language(Some(key.clone()), cx)
                })),
            );
        }

        let view = cx.entity();
        dropdown(list)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_language_menu(cx));
            })
            .into_any_element()
    }

    pub(super) fn voice_group_header(
        &self,
        name: &str,
        count: usize,
        palette: &ForgePalette,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_LABEL)
                    .text_color(palette.text_muted)
                    .child(name.to_owned()),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(count.to_string()),
            )
            .into_any_element()
    }
}
