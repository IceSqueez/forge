use crate::presentation::ActivePresentation;
use forge_types::{datetime_from_text, datetime_to_text};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, EventEmitter, Pixels, SharedString, Window,
    div, prelude::*, px,
};
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

use crate::buttons::{ghost_button_with_icon, primary_button, secondary_button};
use crate::date_display::fmt_day_period;
use crate::display_formats::{ClockStyle, FirstWeekday};
use crate::display_zone::{DisplayZone, WallTime};
use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, Radius, Spacing, body_family, mono_family,
    radius, spacing,
};
use crate::tr;

const PANEL_W: Pixels = px(284.0);
const CELL: Pixels = px(34.0);
const NAV_BTN: Pixels = px(26.0);
const NAV_GLYPH: Pixels = px(14.0);
const STEPPER_W: Pixels = px(40.0);

const HOURS_PER_DAY: i32 = 24;
const HOURS_PER_HALF_DAY: u8 = 12;
const MINUTES_PER_HOUR: i32 = 60;

const DAYS_PER_WEEK: u8 = 7;

const WEEKDAY_KEYS: [&str; DAYS_PER_WEEK as usize] = [
    "widget_date_weekday_sunday",
    "widget_date_weekday_monday",
    "widget_date_weekday_tuesday",
    "widget_date_weekday_wednesday",
    "widget_date_weekday_thursday",
    "widget_date_weekday_friday",
    "widget_date_weekday_saturday",
];

pub struct DateTimePickerLabels {
    pub now: SharedString,
    pub set: SharedString,
    pub cancel: SharedString,
}

#[derive(Debug, Clone)]
pub enum DateTimePickerEvent {
    Picked(SharedString),
    Dismissed,
}

pub struct DateTimePicker {
    palette: ForgePalette,
    labels: DateTimePickerLabels,
    year: i32,
    month: Month,
    day: u8,
    hour: u8,
    minute: u8,
    first_day: FirstWeekday,
    clock: ClockStyle,
    zone: DisplayZone,
    repeated: Option<RepeatedWallTime>,
    later_in_repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RepeatedWallTime {
    earlier: UtcOffset,
    later: UtcOffset,
}

impl EventEmitter<DateTimePickerEvent> for DateTimePicker {}

impl DateTimePicker {
    pub fn new(
        initial: Option<&str>,
        zone: DisplayZone,
        labels: DateTimePickerLabels,
        palette: ForgePalette,
        _cx: &mut Context<Self>,
    ) -> Self {
        let seed = initial
            .and_then(datetime_from_text)
            .unwrap_or_else(OffsetDateTime::now_utc);
        let mut picker = Self {
            palette,
            labels,
            year: seed.year(),
            month: seed.month(),
            day: seed.day(),
            hour: seed.hour(),
            minute: seed.minute(),
            first_day: FirstWeekday::Monday,
            clock: ClockStyle::TwentyFourHour,
            zone,
            repeated: None,
            later_in_repeat: false,
        };
        picker.show_local(seed);
        picker
    }

    pub fn first_day(mut self, first_day: FirstWeekday) -> Self {
        self.first_day = first_day;
        self
    }

    pub fn clock(mut self, clock: ClockStyle) -> Self {
        self.clock = clock;
        self
    }

    fn show_local(&mut self, instant: OffsetDateTime) {
        let wall = self.zone.wall_clock(instant);
        self.show_wall(wall);
        self.later_in_repeat = false;
        self.settle(true);
        self.later_in_repeat = self
            .repeated
            .is_some_and(|repeated| repeated.later == wall.offset());
    }

    fn show_wall(&mut self, wall: OffsetDateTime) {
        self.year = wall.year();
        self.month = wall.month();
        self.day = wall.day();
        self.hour = wall.hour();
        self.minute = wall.minute();
    }

    fn wall(&self) -> Option<PrimitiveDateTime> {
        let date = Date::from_calendar_date(self.year, self.month, self.day).ok()?;
        let time = Time::from_hms(self.hour, self.minute, 0).ok()?;
        Some(PrimitiveDateTime::new(date, time))
    }

    fn wall_time(&self) -> Option<WallTime> {
        self.zone.resolve(self.wall()?)
    }

    fn settle(&mut self, forward: bool) {
        let resolved = self.wall_time();
        if let Some(WallTime::Skipped {
            first_valid,
            last_before,
        }) = resolved
        {
            self.show_wall(if forward { first_valid } else { last_before });
        }
        let repeated = match resolved {
            Some(WallTime::Repeated { earlier, later }) => Some(RepeatedWallTime {
                earlier: earlier.offset(),
                later: later.offset(),
            }),
            _ => None,
        };
        if repeated.is_none() {
            self.later_in_repeat = false;
        }
        self.repeated = repeated;
    }

    fn choose_repeat(&mut self, later: bool, cx: &mut Context<Self>) {
        self.later_in_repeat = later;
        cx.notify();
    }

    fn wall_time_label(&self) -> String {
        match self.clock {
            ClockStyle::TwentyFourHour => format!("{:02}:{:02}", self.hour, self.minute),
            ClockStyle::TwelveHour => format!(
                "{}:{:02} {}",
                self.displayed_hour(),
                self.minute,
                fmt_day_period(day_period_sample(self.is_after_noon()))
            ),
        }
    }

    fn repeat_choice_label(&self, offset: UtcOffset) -> String {
        format!("{} ({})", self.wall_time_label(), offset_label(offset))
    }

    fn is_after_noon(&self) -> bool {
        self.hour >= HOURS_PER_HALF_DAY
    }

    fn displayed_hour(&self) -> u8 {
        match self.clock {
            ClockStyle::TwentyFourHour => self.hour,
            ClockStyle::TwelveHour => match self.hour % HOURS_PER_HALF_DAY {
                0 => HOURS_PER_HALF_DAY,
                hour => hour,
            },
        }
    }

    fn weekday_keys(&self) -> impl Iterator<Item = &'static str> {
        let start = usize::from(self.first_day.days_from_monday() + 1);
        WEEKDAY_KEYS
            .iter()
            .cycle()
            .skip(start)
            .take(WEEKDAY_KEYS.len())
            .copied()
    }

    fn leading_blanks(&self) -> u8 {
        Date::from_calendar_date(self.year, self.month, 1)
            .map(|d| {
                (d.weekday().number_days_from_monday() + DAYS_PER_WEEK
                    - self.first_day.days_from_monday())
                    % DAYS_PER_WEEK
            })
            .unwrap_or(0)
    }

    fn clamp_day(&mut self) {
        let len = self.month.length(self.year);
        if self.day > len {
            self.day = len;
        }
    }

    fn prev_month(&mut self, cx: &mut Context<Self>) {
        if self.month == Month::January {
            self.year -= 1;
        }
        self.month = self.month.previous();
        self.clamp_day();
        self.settle(true);
        cx.notify();
    }

    fn next_month(&mut self, cx: &mut Context<Self>) {
        if self.month == Month::December {
            self.year += 1;
        }
        self.month = self.month.next();
        self.clamp_day();
        self.settle(true);
        cx.notify();
    }

    fn pick_day(&mut self, day: u8, cx: &mut Context<Self>) {
        self.day = day;
        self.settle(true);
        cx.notify();
    }

    fn step_hour(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.hour = (i32::from(self.hour) + delta).rem_euclid(HOURS_PER_DAY) as u8;
        self.settle(delta >= 0);
        cx.notify();
    }

    fn set_day_period(&mut self, after_noon: bool, cx: &mut Context<Self>) {
        if self.is_after_noon() != after_noon {
            self.hour = (self.hour + HOURS_PER_HALF_DAY) % HOURS_PER_DAY as u8;
        }
        self.settle(true);
        cx.notify();
    }

    fn step_minute(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.minute = (i32::from(self.minute) + delta).rem_euclid(MINUTES_PER_HOUR) as u8;
        self.settle(delta >= 0);
        cx.notify();
    }

    fn set_now(&mut self, cx: &mut Context<Self>) {
        self.show_local(OffsetDateTime::now_utc());
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(value) = self.picked_instant() {
            cx.emit(DateTimePickerEvent::Picked(datetime_to_text(&value).into()));
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(DateTimePickerEvent::Dismissed);
    }

    fn picked_instant(&self) -> Option<OffsetDateTime> {
        match self.wall_time()? {
            WallTime::Exact(instant) => Some(instant),
            WallTime::Repeated { earlier, later } => {
                Some(if self.later_in_repeat { later } else { earlier })
            }
            WallTime::Skipped { first_valid, .. } => Some(first_valid),
        }
    }

    fn render_month_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(nav_btn(
                "forge-dtp-prev",
                Icon::ChevronLeft,
                &p,
                cx.listener(|this, _: &ClickEvent, _, cx| this.prev_month(cx)),
            ))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!("{} {}", month_name(self.month), self.year)),
            )
            .child(nav_btn(
                "forge-dtp-next",
                Icon::ChevronRight,
                &p,
                cx.listener(|this, _: &ClickEvent, _, cx| this.next_month(cx)),
            ))
            .into_any_element()
    }

    fn render_weekday_header(&self) -> AnyElement {
        let p = self.palette;
        let mut row = div().flex();
        for key in self.weekday_keys() {
            row = row.child(
                div()
                    .w(CELL)
                    .flex()
                    .justify_center()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(p.text_faint)
                    .child(tr!(key)),
            );
        }
        row.into_any_element()
    }

    fn render_day_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let lead = self.leading_blanks();
        let len = self.month.length(self.year);

        let mut grid = div().flex().flex_wrap().w(CELL * f32::from(DAYS_PER_WEEK));
        for _ in 0..lead {
            grid = grid.child(div().size(CELL));
        }
        for day in 1..=len {
            let selected = day == self.day;
            let hover_bg = p.surface_overlay;
            grid = grid.child(
                div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "forge-dtp-day-{day}"
                    ))))
                    .size(CELL)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius(Radius::Sm))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .cursor_pointer()
                    .when(selected, |c| c.bg(p.brand).text_color(p.base))
                    .when(!selected, |c| {
                        c.text_color(p.text_primary).hover(move |s| s.bg(hover_bg))
                    })
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_day(day, cx)),
                    )
                    .child(day.to_string()),
            );
        }
        grid.into_any_element()
    }

    fn render_time(&self, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let p = self.palette;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Sm, density))
            .child(self.render_stepper(
                "hour",
                self.displayed_hour(),
                cx.listener(|this, _: &ClickEvent, _, cx| this.step_hour(1, cx)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.step_hour(-1, cx)),
                density,
            ))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_faint)
                    .child(":"),
            )
            .child(self.render_stepper(
                "minute",
                self.minute,
                cx.listener(|this, _: &ClickEvent, _, cx| this.step_minute(1, cx)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.step_minute(-1, cx)),
                density,
            ))
            .when(self.clock == ClockStyle::TwelveHour, |row| {
                row.child(self.render_day_period(cx))
            })
            .into_any_element()
    }

    fn render_day_period(&self, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let after_noon = self.is_after_noon();
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(self.render_day_period_option(false, !after_noon, cx))
            .child(self.render_day_period_option(true, after_noon, cx))
            .into_any_element()
    }

    fn render_day_period_option(
        &self,
        after_noon: bool,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let id = if after_noon {
            "forge-dtp-after-noon"
        } else {
            "forge-dtp-before-noon"
        };
        choice_chip(
            id,
            fmt_day_period(day_period_sample(after_noon)),
            selected,
            &self.palette,
            density,
        )
        .w(STEPPER_W)
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _, cx| this.set_day_period(after_noon, cx)),
        )
        .into_any_element()
    }

    fn render_repeat_choices(
        &self,
        repeated: RepeatedWallTime,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        div()
            .w_full()
            .flex()
            .gap(spacing(Spacing::Xs, density))
            .child(
                choice_chip(
                    "forge-dtp-repeat-earlier",
                    self.repeat_choice_label(repeated.earlier),
                    !self.later_in_repeat,
                    &self.palette,
                    density,
                )
                .flex_1()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.choose_repeat(false, cx))),
            )
            .child(
                choice_chip(
                    "forge-dtp-repeat-later",
                    self.repeat_choice_label(repeated.later),
                    self.later_in_repeat,
                    &self.palette,
                    density,
                )
                .flex_1()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.choose_repeat(true, cx))),
            )
            .into_any_element()
    }

    fn render_stepper(
        &self,
        id: &str,
        value: u8,
        on_up: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_down: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        density: Density,
    ) -> AnyElement {
        let p = self.palette;
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(nav_btn(
                ElementId::Name(SharedString::from(format!("forge-dtp-{id}-up"))),
                Icon::ChevronUp,
                &p,
                on_up,
            ))
            .child(
                div()
                    .w(STEPPER_W)
                    .flex()
                    .justify_center()
                    .font_family(mono_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!("{value:02}")),
            )
            .child(nav_btn(
                ElementId::Name(SharedString::from(format!("forge-dtp-{id}-down"))),
                Icon::ChevronDown,
                &p,
                on_down,
            ))
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let p = self.palette;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, density))
            .child(
                ghost_button_with_icon(Icon::Clock, self.labels.now.clone(), &p).on_click(
                    "forge-dtp-now",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_now(cx)),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(secondary_button(self.labels.cancel.clone(), &p).on_click(
                        "forge-dtp-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss(cx)),
                    ))
                    .child(primary_button(self.labels.set.clone(), &p).on_click(
                        "forge-dtp-set",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                    )),
            )
            .into_any_element()
    }
}

impl Render for DateTimePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let p = self.palette;
        div()
            .occlude()
            .w(PANEL_W)
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Sm, density))
            .bg(p.elevated)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(p.border_input)
            .child(self.render_month_nav(cx))
            .child(self.render_weekday_header())
            .child(self.render_day_grid(cx))
            .child(div().w_full().h(BORDER_THIN).bg(p.border_regular))
            .child(self.render_time(cx))
            .when_some(self.repeated, |panel, repeated| {
                panel.child(self.render_repeat_choices(repeated, cx))
            })
            .child(self.render_footer(cx))
    }
}

fn nav_btn(
    id: impl Into<ElementId>,
    glyph: Icon,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let hover = palette.surface_overlay;
    let ink = palette.text_secondary;
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .size(NAV_BTN)
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(handler)
        .child(icon(glyph, NAV_GLYPH, ink))
        .into_any_element()
}

fn day_period_sample(after_noon: bool) -> Time {
    let hour = if after_noon { HOURS_PER_HALF_DAY } else { 0 };
    Time::from_hms(hour, 0, 0).unwrap_or(Time::MIDNIGHT)
}

fn choice_chip(
    id: &'static str,
    label: String,
    selected: bool,
    palette: &ForgePalette,
    density: Density,
) -> gpui::Stateful<gpui::Div> {
    let p = *palette;
    let hover_bg = p.surface_overlay;
    div()
        .id(id)
        .flex()
        .justify_center()
        .py(spacing(Spacing::Xxs, density))
        .rounded(radius(Radius::Sm))
        .font_family(mono_family())
        .text_size(FONT_XS)
        .cursor_pointer()
        .when(selected, |c| c.bg(p.brand).text_color(p.base))
        .when(!selected, |c| {
            c.text_color(p.text_secondary)
                .hover(move |s| s.bg(hover_bg))
        })
        .child(label)
}

fn offset_label(offset: UtcOffset) -> String {
    let sign = if offset.is_negative() { '-' } else { '+' };
    format!(
        "{sign}{:02}:{:02}",
        offset.whole_hours().unsigned_abs(),
        offset.minutes_past_hour().unsigned_abs()
    )
}

fn month_name(month: Month) -> String {
    let key = match month {
        Month::January => "widget_date_month_january",
        Month::February => "widget_date_month_february",
        Month::March => "widget_date_month_march",
        Month::April => "widget_date_month_april",
        Month::May => "widget_date_month_may",
        Month::June => "widget_date_month_june",
        Month::July => "widget_date_month_july",
        Month::August => "widget_date_month_august",
        Month::September => "widget_date_month_september",
        Month::October => "widget_date_month_october",
        Month::November => "widget_date_month_november",
        Month::December => "widget_date_month_december",
    };
    tr!(key)
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use fluent::{FluentBundle, FluentResource};
    use unic_langid::LanguageIdentifier;

    use super::*;
    use crate::locale::install_bundle;

    const UK_CATALOG: &str = include_str!("../locales/uk/main.ftl");

    fn install_ukrainian() {
        let lang: LanguageIdentifier = "uk-UA".parse().unwrap_or_default();
        let resource = FluentResource::try_new(UK_CATALOG.to_owned()).unwrap_or_else(|(r, _)| r);
        let mut bundle = FluentBundle::new(vec![lang]);
        assert!(bundle.add_resource(resource).is_ok());
        install_bundle(Rc::new(bundle));
    }

    #[test]
    fn month_names_follow_the_installed_language_in_calendar_order() {
        install_ukrainian();
        let months = [
            Month::January,
            Month::February,
            Month::March,
            Month::April,
            Month::May,
            Month::June,
            Month::July,
            Month::August,
            Month::September,
            Month::October,
            Month::November,
            Month::December,
        ]
        .map(month_name);

        assert_eq!(
            months,
            [
                "Січень",
                "Лютий",
                "Березень",
                "Квітень",
                "Травень",
                "Червень",
                "Липень",
                "Серпень",
                "Вересень",
                "Жовтень",
                "Листопад",
                "Грудень",
            ]
        );
    }

    fn picker(year: i32, month: Month, first_day: FirstWeekday) -> DateTimePicker {
        DateTimePicker {
            palette: crate::palette::FORGE_DEFAULT,
            labels: DateTimePickerLabels {
                now: SharedString::default(),
                set: SharedString::default(),
                cancel: SharedString::default(),
            },
            year,
            month,
            day: 1,
            hour: 0,
            minute: 0,
            first_day,
            clock: ClockStyle::TwentyFourHour,
            zone: DisplayZone::fixed(UtcOffset::UTC),
            repeated: None,
            later_in_repeat: false,
        }
    }

    fn at_time(clock: ClockStyle, hour: u8, minute: u8) -> DateTimePicker {
        let mut at = picker(2026, Month::October, FirstWeekday::Monday);
        at.clock = clock;
        at.hour = hour;
        at.minute = minute;
        at
    }

    fn zone(name: &str) -> DisplayZone {
        DisplayZone::named(name).unwrap()
    }

    fn instant(text: &str) -> OffsetDateTime {
        datetime_from_text(text).unwrap()
    }

    fn showing(zone: DisplayZone, seed: &str) -> DateTimePicker {
        let mut shown = picker(2000, Month::January, FirstWeekday::Monday);
        shown.zone = zone;
        shown.show_local(instant(seed));
        shown
    }

    fn after(
        cx: &mut gpui::TestAppContext,
        start: DateTimePicker,
        act: impl FnOnce(&mut DateTimePicker, &mut Context<DateTimePicker>),
    ) -> DateTimePicker {
        let entity = cx.update(|cx| cx.new(|_| start));
        entity.update(cx, act);
        entity.read_with(cx, |p, _| DateTimePicker {
            labels: DateTimePickerLabels {
                now: p.labels.now.clone(),
                set: p.labels.set.clone(),
                cancel: p.labels.cancel.clone(),
            },
            zone: p.zone.clone(),
            ..*p
        })
    }

    fn hm(p: &DateTimePicker) -> (u8, u8) {
        (p.hour, p.minute)
    }

    #[test]
    fn twelve_hour_clock_shows_midnight_and_noon_as_twelve_and_other_hours_as_one_to_eleven() {
        let shown = [0, 1, 11, 12, 13, 23]
            .map(|hour| at_time(ClockStyle::TwelveHour, hour, 0).displayed_hour());
        assert_eq!(shown, [12, 1, 11, 12, 1, 11]);
    }

    #[test]
    fn twenty_four_hour_clock_shows_the_stored_hour() {
        let shown =
            [0, 12, 23].map(|hour| at_time(ClockStyle::TwentyFourHour, hour, 0).displayed_hour());
        assert_eq!(shown, [0, 12, 23]);
    }

    #[gpui::test]
    fn twelve_hour_stepper_moves_one_wall_clock_hour_across_the_noon_and_midnight_boundaries(
        cx: &mut gpui::TestAppContext,
    ) {
        for (start, delta, expected) in [
            (11, 1, 12),
            (12, -1, 11),
            (23, 1, 0),
            (0, -1, 23),
            (5, 1, 6),
        ] {
            let stepped = after(cx, at_time(ClockStyle::TwelveHour, start, 30), |p, cx| {
                p.step_hour(delta, cx)
            });
            assert_eq!(hm(&stepped), (expected, 30), "start {start} delta {delta}");
        }
    }

    #[gpui::test]
    fn twenty_four_hour_stepper_wraps_between_twenty_three_and_zero(cx: &mut gpui::TestAppContext) {
        for (start, delta, expected) in [(23, 1, 0), (0, -1, 23), (11, 1, 12)] {
            let stepped = after(
                cx,
                at_time(ClockStyle::TwentyFourHour, start, 30),
                |p, cx| p.step_hour(delta, cx),
            );
            assert_eq!(hm(&stepped), (expected, 30), "start {start} delta {delta}");
        }
    }

    #[gpui::test]
    fn day_period_toggle_moves_twelve_hours_and_keeps_the_minutes(cx: &mut gpui::TestAppContext) {
        for (start, after_noon, expected) in
            [(0, true, 12), (12, false, 0), (9, true, 21), (21, false, 9)]
        {
            let moved = after(cx, at_time(ClockStyle::TwelveHour, start, 47), |p, cx| {
                p.set_day_period(after_noon, cx)
            });
            assert_eq!(
                hm(&moved),
                (expected, 47),
                "start {start} after_noon {after_noon}"
            );
        }
    }

    #[gpui::test]
    fn choosing_the_day_period_already_shown_leaves_the_time_unchanged(
        cx: &mut gpui::TestAppContext,
    ) {
        for (start, after_noon) in [(0, false), (11, false), (12, true), (23, true)] {
            let kept = after(cx, at_time(ClockStyle::TwelveHour, start, 15), |p, cx| {
                p.set_day_period(after_noon, cx)
            });
            assert_eq!(
                hm(&kept),
                (start, 15),
                "start {start} after_noon {after_noon}"
            );
        }
    }

    #[test]
    fn picked_text_reads_back_as_the_seeded_instant_in_every_zone() {
        for (name, seed) in [
            ("Europe/Kyiv", "2026-10-21T14:30:00+03:00"),
            ("Europe/Kyiv", "2026-03-29T01:00:00Z"),
            ("Europe/Kyiv", "2026-03-29T00:59:00Z"),
            ("Europe/Kyiv", "2026-10-25T00:30:00Z"),
            ("Europe/Kyiv", "2026-10-25T01:30:00Z"),
            ("America/New_York", "2026-10-21T14:30:00-05:00"),
            ("America/New_York", "2026-11-01T05:30:00Z"),
            ("America/New_York", "2026-11-01T06:30:00Z"),
            ("America/New_York", "2026-12-31T23:59:00-05:00"),
            ("UTC", "2026-10-21T14:30:00+00:00"),
        ] {
            let picked = datetime_to_text(&showing(zone(name), seed).picked_instant().unwrap());
            assert_eq!(
                datetime_from_text(&picked),
                Some(instant(seed)),
                "{name} seed {seed} picked {picked}"
            );
        }
    }

    #[test]
    fn picked_text_is_written_in_the_zone_offset_of_the_shown_wall_time() {
        for (name, seed, expected) in [
            (
                "Europe/Kyiv",
                "2026-10-21T11:30:00Z",
                "2026-10-21T14:30:00+03:00",
            ),
            (
                "Europe/Kyiv",
                "2026-12-01T12:30:00Z",
                "2026-12-01T14:30:00+02:00",
            ),
            (
                "America/New_York",
                "2026-10-21T19:30:00Z",
                "2026-10-21T15:30:00-04:00",
            ),
            (
                "America/New_York",
                "2026-12-01T19:30:00Z",
                "2026-12-01T14:30:00-05:00",
            ),
        ] {
            let picked = showing(zone(name), seed).picked_instant().unwrap();
            assert_eq!(datetime_to_text(&picked), expected, "{name} seed {seed}");
        }
    }

    #[test]
    fn seeding_either_instant_of_a_repeated_hour_shows_the_same_wall_time_with_its_choice() {
        for (seed, later) in [
            ("2026-10-25T00:30:00Z", false),
            ("2026-10-25T01:30:00Z", true),
        ] {
            let shown = showing(zone("Europe/Kyiv"), seed);
            assert_eq!(
                (hm(&shown), shown.later_in_repeat, shown.repeated.is_some()),
                ((3, 30), later, true),
                "seed {seed}"
            );
        }
    }

    #[gpui::test]
    fn both_repeated_hour_choices_emit_their_own_instant(cx: &mut gpui::TestAppContext) {
        for (name, seed, earlier, later) in [
            (
                "Europe/Kyiv",
                "2026-10-25T00:30:00Z",
                "2026-10-25T03:30:00+03:00",
                "2026-10-25T03:30:00+02:00",
            ),
            (
                "America/New_York",
                "2026-11-01T05:30:00Z",
                "2026-11-01T01:30:00-04:00",
                "2026-11-01T01:30:00-05:00",
            ),
        ] {
            let picked = [false, true].map(|choice| {
                let chosen = after(cx, showing(zone(name), seed), |p, cx| {
                    p.choose_repeat(choice, cx)
                });
                datetime_to_text(&chosen.picked_instant().unwrap())
            });
            assert_eq!(picked, [earlier, later], "{name}");
        }
    }

    #[test]
    fn repeated_hour_choices_are_labelled_with_their_offsets() {
        let shown = showing(zone("Europe/Kyiv"), "2026-10-25T00:30:00Z");
        let repeated = shown.repeated.unwrap();
        assert_eq!(
            [
                shown.repeat_choice_label(repeated.earlier),
                shown.repeat_choice_label(repeated.later)
            ],
            ["03:30 (+03:00)", "03:30 (+02:00)"]
        );
    }

    #[gpui::test]
    fn stepping_out_of_a_repeated_hour_forgets_the_later_choice(cx: &mut gpui::TestAppContext) {
        let left = after(
            cx,
            showing(zone("Europe/Kyiv"), "2026-10-25T01:30:00Z"),
            |p, cx| p.step_hour(1, cx),
        );
        assert_eq!(
            (hm(&left), left.repeated, left.later_in_repeat),
            ((4, 30), None, false)
        );
    }

    #[gpui::test]
    fn stepping_the_hour_forward_into_a_skipped_spring_time_jumps_to_the_first_valid_minute(
        cx: &mut gpui::TestAppContext,
    ) {
        for (name, seed, expected) in [
            (
                "Europe/Kyiv",
                "2026-03-29T00:30:00Z",
                "2026-03-29T04:00:00+03:00",
            ),
            (
                "America/New_York",
                "2026-03-08T06:30:00Z",
                "2026-03-08T03:00:00-04:00",
            ),
        ] {
            let stepped = after(cx, showing(zone(name), seed), |p, cx| p.step_hour(1, cx));
            assert_eq!(
                datetime_to_text(&stepped.picked_instant().unwrap()),
                expected,
                "{name} seed {seed}"
            );
        }
    }

    #[gpui::test]
    fn stepping_the_hour_back_into_a_skipped_spring_time_lands_on_the_last_valid_minute(
        cx: &mut gpui::TestAppContext,
    ) {
        for (name, seed, expected) in [
            (
                "Europe/Kyiv",
                "2026-03-29T01:30:00Z",
                "2026-03-29T02:59:00+02:00",
            ),
            (
                "America/New_York",
                "2026-03-08T07:30:00Z",
                "2026-03-08T01:59:00-05:00",
            ),
        ] {
            let stepped = after(cx, showing(zone(name), seed), |p, cx| p.step_hour(-1, cx));
            assert_eq!(
                datetime_to_text(&stepped.picked_instant().unwrap()),
                expected,
                "{name} seed {seed}"
            );
        }
    }

    #[gpui::test]
    fn picking_the_spring_forward_day_at_a_skipped_time_moves_to_the_first_valid_minute(
        cx: &mut gpui::TestAppContext,
    ) {
        let picked = after(
            cx,
            showing(zone("Europe/Kyiv"), "2026-03-28T01:30:00Z"),
            |p, cx| p.pick_day(29, cx),
        );
        assert_eq!(
            datetime_to_text(&picked.picked_instant().unwrap()),
            "2026-03-29T04:00:00+03:00"
        );
    }

    #[test]
    fn weekday_headers_start_on_the_chosen_first_day_in_the_installed_language() {
        install_ukrainian();
        for (first_day, expected) in [
            (
                FirstWeekday::Monday,
                ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Нд"],
            ),
            (
                FirstWeekday::Sunday,
                ["Нд", "Пн", "Вт", "Ср", "Чт", "Пт", "Сб"],
            ),
            (
                FirstWeekday::Saturday,
                ["Сб", "Нд", "Пн", "Вт", "Ср", "Чт", "Пт"],
            ),
        ] {
            let headers: Vec<String> = picker(2026, Month::October, first_day)
                .weekday_keys()
                .map(|key| tr!(key))
                .collect();
            assert_eq!(headers, expected, "{first_day:?}");
        }
    }

    #[test]
    fn day_grid_offsets_the_first_of_the_month_from_the_chosen_first_day() {
        for (year, month, monday_first, sunday_first, saturday_first) in [
            (2026, Month::October, 3, 4, 5),
            (2026, Month::June, 0, 1, 2),
            (2026, Month::February, 6, 0, 1),
            (2026, Month::August, 5, 6, 0),
        ] {
            let blanks = [
                FirstWeekday::Monday,
                FirstWeekday::Sunday,
                FirstWeekday::Saturday,
            ]
            .map(|first_day| picker(year, month, first_day).leading_blanks());
            assert_eq!(
                blanks,
                [monday_first, sunday_first, saturday_first],
                "{year}-{month}"
            );
        }
    }
}
