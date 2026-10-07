use std::time::Instant;

use forge_components::{
    Density, FONT_XXS, ForgePalette, Icon, Radius, empty_state, icon, mono_family, pad_tile,
    radius, tooltip_builder, tr, with_alpha,
};
use gpui::{
    AnyElement, ClickEvent, Context, Pixels, Rgba, SharedString, Window, div, prelude::*, px, svg,
};

use super::adoption::{LibraryBadge, library_badge};
use super::categories::category_color;
use super::playback::{readout_total, time_readout};
use super::{SoundClip, SoundboardView};
use crate::presentation::ActivePresentation;

pub(super) const GRID_GAP: Pixels = px(10.0);
const PADS_PER_ROW: usize = 4;
pub(super) const PAD_GLYPH: Pixels = px(15.0);
pub(super) const LOOP_ICON: Pixels = px(10.0);
const PROGRESS_WIDTH: f32 = 0.46;
const PAD_ACTION_TILE: Pixels = px(20.0);
const PAD_ACTION_GLYPH: Pixels = px(13.0);
const PAD_ACTION_GAP: Pixels = px(3.0);
const PAD_ACTION_HOVER_ALPHA: f32 = 0.16;

impl SoundboardView {
    fn render_pad(
        &self,
        index: usize,
        clip: &SoundClip,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = clip.id;
        let progress = self.playing.get(&id);
        let playing = progress.is_some();
        let color = category_color(&clip.category, palette);
        let glyph = if playing {
            Icon::PlayerPause
        } else {
            clip.glyph
        };

        let hotkey_badge = clip
            .hotkey
            .as_deref()
            .map(|hk| self.render_hotkey_badge(index, id, hk, palette));

        let edit_btn = self.pad_action_button(
            ("sb-pad-edit", index),
            format!("sb-pad-edit-{index}").into(),
            Icon::Pencil,
            palette.brand,
            move |this, _ev, window, cx| this.open_edit(id, window, cx),
            cx,
        );
        let delete_btn = self.pad_action_button(
            ("sb-pad-del", index),
            format!("sb-pad-del-{index}").into(),
            Icon::Trash,
            palette.random,
            move |this, _ev, _window, cx| this.request_delete(id, cx),
            cx,
        );
        let top_right = div()
            .flex()
            .items_center()
            .gap(PAD_ACTION_GAP)
            .child(edit_btn)
            .child(delete_btn)
            .children(hotkey_badge);

        let elapsed_secs = progress.map_or(0.0, |p| {
            Instant::now()
                .saturating_duration_since(p.started_at)
                .as_secs_f64()
        });
        let total = readout_total(clip.duration_secs, progress);
        let readout = if !playing && total.is_none() {
            "\u{2014}".to_owned()
        } else {
            time_readout(elapsed_secs, total)
        };
        let readout_color = if playing { color } else { palette.text_faint };

        let missing = self.source_is_missing(id);
        let mut status = div().flex().flex_1().min_w_0().items_center().gap(px(5.0));
        if clip.loop_playback {
            status = status.child(icon(Icon::Repeat, LOOP_ICON, palette.text_faint));
        }
        if missing {
            status = status
                .child(icon(Icon::AlertTriangle, LOOP_ICON, palette.warning))
                .child(
                    div()
                        .flex_none()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.warning)
                        .child(tr!("soundboard_pad_source_missing")),
                );
        }
        match library_badge(self.availability.get(&id)) {
            Some(LibraryBadge::Blocked(reason)) => {
                status = status
                    .child(icon(Icon::AlertTriangle, LOOP_ICON, palette.warning))
                    .child(
                        div()
                            .id(("sb-pad-refused", index))
                            .flex_none()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.warning)
                            .tooltip(tooltip_builder(reason, palette))
                            .child(tr!("soundboard_pad_adopt_blocked")),
                    );
            }
            Some(LibraryBadge::Pending) => {
                status = status.child(
                    div()
                        .flex_none()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(tr!("soundboard_pad_not_in_library")),
                );
            }
            None => {}
        }
        if playing {
            status = status.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(color)
                    .child(tr!("soundboard_pad_playing")),
            );
        }
        let sublabel = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(5.0))
            .mt(px(3.0))
            .child(status)
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(readout_color)
                    .child(readout),
            );

        let glyph_color = if missing { palette.warning } else { color };
        let mut pad = pad_tile(
            (gpui::ElementId::from("sb-pad"), id.to_string()),
            icon(glyph, PAD_GLYPH, glyph_color),
            clip.name.clone(),
            palette,
        )
        .top_right(top_right)
        .sublabel(sublabel)
        .selected(playing)
        .accent(color)
        .hover_border(color)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_play(id, cx)));

        if let Some(prog) = progress {
            let total = if prog.looped {
                None
            } else {
                prog.duration_secs.filter(|d| *d > 0.0)
            };
            let fraction = match total {
                Some(total) => (elapsed_secs / total).clamp(0.0, 1.0) as f32,
                None => PROGRESS_WIDTH,
            };
            pad = pad.progress(fraction, color);
        }
        if !missing {
            return pad.into_any_element();
        }
        div()
            .id(("sb-pad-missing", index))
            .w_full()
            .tooltip(tooltip_builder(
                tr!("soundboard_pad_source_missing_hint"),
                palette,
            ))
            .child(pad)
            .into_any_element()
    }

    fn pad_action_button(
        &self,
        id: impl Into<gpui::ElementId>,
        group: SharedString,
        glyph: Icon,
        tint: Rgba,
        handler: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let idle = cx.palette().text_faint;
        div()
            .id(id.into())
            .group(group.clone())
            .flex()
            .items_center()
            .justify_center()
            .size(PAD_ACTION_TILE)
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(with_alpha(tint, PAD_ACTION_HOVER_ALPHA)))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                cx.stop_propagation();
                handler(this, ev, window, cx);
            }))
            .child(
                svg()
                    .flex_none()
                    .size(PAD_ACTION_GLYPH)
                    .path(glyph.path())
                    .text_color(idle)
                    .group_hover(group, move |s| s.text_color(tint)),
            )
            .into_any_element()
    }

    pub(super) fn render_grid(&self, elements: Vec<AnyElement>) -> AnyElement {
        let mut grid = div().w_full().flex().flex_col().gap(GRID_GAP);
        let mut iter = elements.into_iter().peekable();
        while iter.peek().is_some() {
            let mut row = div().w_full().flex().flex_row().gap(GRID_GAP);
            for _ in 0..PADS_PER_ROW {
                match iter.next() {
                    Some(el) => row = row.child(div().flex_1().min_w_0().child(el)),
                    None => row = row.child(div().flex_1()),
                }
            }
            grid = grid.child(row);
        }
        grid.into_any_element()
    }

    pub(super) fn render_pads(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            let message = if self.clips.is_empty() {
                tr!("soundboard_empty_title")
            } else {
                tr!("soundboard_no_matches")
            };
            return empty_state(message, palette)
                .glyph(Icon::Music)
                .density(density)
                .into_any_element();
        }
        let pads: Vec<AnyElement> = indices
            .into_iter()
            .map(|i| self.render_pad(i, &self.clips[i], palette, cx))
            .collect();
        self.render_grid(pads)
    }
}
