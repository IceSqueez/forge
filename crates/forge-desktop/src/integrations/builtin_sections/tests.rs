use std::cell::RefCell;

use forge_components::FORGE_DEFAULT;
use gpui::{Modifiers, TestAppContext, size};

use super::*;

type Fired = Rc<RefCell<Vec<Vec<(String, String)>>>>;

const WINDOW_W: f32 = 720.0;
const WINDOW_H: f32 = 360.0;

fn step(kind_id: &str) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: Default::default(),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn row(name: &str, active: bool, on_click: Option<&str>) -> ContentListItem {
    ContentListItem {
        icon: SectionIcon::new("film"),
        icon_tint: None,
        name: name.to_owned(),
        name_is_identifier: false,
        active,
        active_label: None,
        trailing: Vec::new(),
        enabled: true,
        on_click: on_click.map(step),
    }
}

fn with_action_glyph(mut item: ContentListItem, kind_id: &str) -> ContentListItem {
    item.trailing.push(TrailingToken::ActionIcon {
        icon: SectionIcon::new("eye"),
        tint: Tone::Positive,
        step: step(kind_id),
    });
    item
}

fn list(title: &str, items: Vec<ContentListItem>) -> ContentList {
    ContentList {
        title: forge_types::VerbatimText::new(title).into(),
        icon: SectionIcon::new("film"),
        icon_tint: None,
        inline_label: None,
        count_label: None,
        visible_rows: None,
        row_spacing: RowSpacing::Standard,
        refreshable: false,
        items: items.into(),
        footer: None,
    }
}

struct Host {
    sections: Vec<DetailSection>,
    fired: Fired,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fired = Rc::clone(&self.fired);
        let run_step: SectionStepRun = Rc::new(move |click: &StepClick, _, _| {
            if let Some(current) = fired.borrow_mut().last_mut() {
                current.push((click.row.to_string(), click.step.kind_id.clone()));
            }
        });
        let hooks = SectionHooks {
            refresh: None,
            run_step: Some(run_step),
        };
        div().size_full().child(content_sections(
            &self.sections,
            &hooks,
            &FORGE_DEFAULT,
            Density::default(),
            cx,
        ))
    }
}

fn clickable_parts(list: &ContentList) -> Vec<&'static str> {
    let hooks = RowHooks {
        scope: content_list_scope(list),
        pad_y: 0.0,
        run_step: None,
    };
    let mut parts = Vec::new();
    for (row_index, item) in list.items.iter().enumerate() {
        parts.push(hooks.element_id("row", row_index));
        for index in 0..item.trailing.len() {
            parts.push(hooks.element_id(&format!("token-{index}"), row_index));
        }
    }
    parts
        .into_iter()
        .map(|part| &*part.to_string().leak())
        .collect()
}

fn click_every_part(left: ContentList, right: ContentList, cx: &mut TestAppContext) -> Fired {
    let parts: Vec<&'static str> = [&left, &right]
        .into_iter()
        .flat_map(clickable_parts)
        .collect();
    let fired: Fired = Rc::default();
    let host_fired = Rc::clone(&fired);
    crate::test_support::install_presentation(cx);
    let (_host, vcx) = cx.add_window_view(|_window, _cx| Host {
        sections: vec![DetailSection::TwoColumnLists {
            left: Box::new(left),
            right: Box::new(right),
        }],
        fired: host_fired,
    });
    vcx.simulate_resize(size(px(WINDOW_W), px(WINDOW_H)));
    vcx.run_until_parked();
    for part in parts {
        fired.borrow_mut().push(Vec::new());
        let center = vcx.debug_bounds(part).expect("the part paints").center();
        vcx.simulate_click(center, Modifiers::none());
    }
    fired
}

fn distinct(fired: &Fired) -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = fired.borrow().iter().flatten().cloned().collect();
    all.sort();
    all.dedup();
    all
}

#[gpui::test]
fn only_rows_and_glyphs_that_carry_a_step_run_it_on_click(cx: &mut TestAppContext) {
    let fired = click_every_part(
        list(
            "Scenes",
            vec![
                row("Live", true, Some("obs.scene.set_current")),
                row("Intro", false, Some("obs.scene.set_current")),
                row("Plain", false, None),
            ],
        ),
        list(
            "Sources",
            vec![with_action_glyph(
                row("Mic", false, None),
                "obs.source.set_visibility",
            )],
        ),
        cx,
    );

    assert_eq!(
        distinct(&fired),
        vec![
            ("Intro".to_owned(), "obs.scene.set_current".to_owned()),
            ("Mic".to_owned(), "obs.source.set_visibility".to_owned()),
        ]
    );
}

#[gpui::test]
fn a_glyph_click_inside_an_actionable_row_runs_only_the_glyph_step(cx: &mut TestAppContext) {
    let fired = click_every_part(
        list(
            "Scenes",
            vec![with_action_glyph(
                row("Cam", false, Some("obs.scene.set_current")),
                "obs.source.set_visibility",
            )],
        ),
        list("Sources", Vec::new()),
        cx,
    );

    assert!(
        distinct(&fired).contains(&("Cam".to_owned(), "obs.source.set_visibility".to_owned())),
        "the glyph never fired"
    );
    let doubled: Vec<Vec<(String, String)>> = fired
        .borrow()
        .iter()
        .filter(|click| click.len() > 1)
        .cloned()
        .collect();
    assert!(
        doubled.is_empty(),
        "one click ran several steps: {doubled:?}"
    );
}

#[gpui::test]
fn equal_row_names_in_two_lists_each_run_their_own_step(cx: &mut TestAppContext) {
    let fired = click_every_part(
        list(
            "Scenes",
            vec![row("Mic", false, Some("obs.scene.set_current"))],
        ),
        list(
            "Sources",
            vec![row("Mic", false, Some("obs.source.set_visibility"))],
        ),
        cx,
    );

    assert_eq!(
        distinct(&fired),
        vec![
            ("Mic".to_owned(), "obs.scene.set_current".to_owned()),
            ("Mic".to_owned(), "obs.source.set_visibility".to_owned()),
        ]
    );
}

#[gpui::test]
fn two_glyphs_on_one_row_each_run_their_own_step(cx: &mut TestAppContext) {
    let source = with_action_glyph(
        with_action_glyph(row("Mic", false, None), "obs.source.set_visibility"),
        "obs.source.set_lock",
    );
    let fired = click_every_part(
        list("Sources", vec![source]),
        list("Scenes", Vec::new()),
        cx,
    );

    assert_eq!(
        distinct(&fired),
        vec![
            ("Mic".to_owned(), "obs.source.set_lock".to_owned()),
            ("Mic".to_owned(), "obs.source.set_visibility".to_owned()),
        ]
    );
}

#[gpui::test]
fn equal_row_names_in_one_list_each_run_only_their_own_step(cx: &mut TestAppContext) {
    let fired = click_every_part(
        list(
            "Sources",
            vec![
                with_action_glyph(
                    row("Mic", false, Some("obs.scene.set_current")),
                    "obs.source.set_visibility",
                ),
                with_action_glyph(
                    row("Mic", false, Some("obs.input.set_mute")),
                    "obs.source.set_lock",
                ),
            ],
        ),
        list("Scenes", Vec::new()),
        cx,
    );

    let doubled: Vec<Vec<(String, String)>> = fired
        .borrow()
        .iter()
        .filter(|click| click.len() > 1)
        .cloned()
        .collect();
    assert!(
        doubled.is_empty(),
        "one click ran several steps: {doubled:?}"
    );
    assert_eq!(
        distinct(&fired),
        vec![
            ("Mic".to_owned(), "obs.input.set_mute".to_owned()),
            ("Mic".to_owned(), "obs.scene.set_current".to_owned()),
            ("Mic".to_owned(), "obs.source.set_lock".to_owned()),
            ("Mic".to_owned(), "obs.source.set_visibility".to_owned()),
        ]
    );
}
