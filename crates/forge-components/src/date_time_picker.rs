use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, EventEmitter, Pixels, SharedString, Window,
    div, prelude::*, px,
};
use time::{Date, Month, OffsetDateTime, Time, format_description::well_known::Rfc3339};

use crate::buttons::{ghost_button_with_icon, primary_button, secondary_button};
use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, HAIRLINE, Radius, Spacing, body_family,
    mono_family, radius, spacing,
};

const PANEL_W: Pixels = px(284.0);
const CELL: Pixels = px(34.0);
const NAV_BTN: Pixels = px(26.0);
const NAV_GLYPH: Pixels = px(14.0);

const WEEKDAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

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
}

impl EventEmitter<DateTimePickerEvent> for DateTimePicker {}

impl DateTimePicker {
    pub fn new(
        initial: Option<&str>,
        labels: DateTimePickerLabels,
        palette: ForgePalette,
        _cx: &mut Context<Self>,
    ) -> Self {
        let seed = initial
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
            .unwrap_or_else(OffsetDateTime::now_utc);
        Self {
            palette,
            labels,
            year: seed.year(),
            month: seed.month(),
            day: seed.day(),
            hour: seed.hour(),
            minute: seed.minute(),
        }
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
        cx.notify();
    }

    fn next_month(&mut self, cx: &mut Context<Self>) {
        if self.month == Month::December {
            self.year += 1;
        }
        self.month = self.month.next();
        self.clamp_day();
        cx.notify();
    }

    fn pick_day(&mut self, day: u8, cx: &mut Context<Self>) {
        self.day = day;
        cx.notify();
    }

    fn step_hour(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.hour = (i32::from(self.hour) + delta).rem_euclid(24) as u8;
        cx.notify();
    }

    fn step_minute(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.minute = (i32::from(self.minute) + delta).rem_euclid(60) as u8;
        cx.notify();
    }

    fn set_now(&mut self, cx: &mut Context<Self>) {
        let now = OffsetDateTime::now_utc();
        self.year = now.year();
        self.month = now.month();
        self.day = now.day();
        self.hour = now.hour();
        self.minute = now.minute();
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(value) = self.rfc3339() {
            cx.emit(DateTimePickerEvent::Picked(value.into()));
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(DateTimePickerEvent::Dismissed);
    }

    /// Emits `YYYY-MM-DDThh:mm:00Z`; the picked wall-clock is treated as UTC so the value round-trips through the runtime's RFC 3339 parser without a local offset.
    fn rfc3339(&self) -> Option<String> {
        let date = Date::from_calendar_date(self.year, self.month, self.day).ok()?;
        let time = Time::from_hms(self.hour, self.minute, 0).ok()?;
        date.with_time(time).assume_utc().format(&Rfc3339).ok()
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
        for label in WEEKDAYS {
            row = row.child(
                div()
                    .w(CELL)
                    .flex()
                    .justify_center()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(p.text_faint)
                    .child(label),
            );
        }
        row.into_any_element()
    }

    fn render_day_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let lead = Date::from_calendar_date(self.year, self.month, 1)
            .map(|d| d.weekday().number_days_from_sunday())
            .unwrap_or(0);
        let len = self.month.length(self.year);

        let mut grid = div().flex().flex_wrap().w(CELL * 7.0);
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
        let p = self.palette;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(self.render_stepper(
                "hour",
                self.hour,
                cx.listener(|this, _: &ClickEvent, _, cx| this.step_hour(1, cx)),
                cx.listener(|this, _: &ClickEvent, _, cx| this.step_hour(-1, cx)),
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
            ))
            .into_any_element()
    }

    fn render_stepper(
        &self,
        id: &str,
        value: u8,
        on_up: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_down: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let p = self.palette;
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(nav_btn(
                ElementId::Name(SharedString::from(format!("forge-dtp-{id}-up"))),
                Icon::ChevronUp,
                &p,
                on_up,
            ))
            .child(
                div()
                    .w(px(40.0))
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
        let p = self.palette;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, Density::Cozy))
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
                    .gap(spacing(Spacing::Xs, Density::Cozy))
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
        let p = self.palette;
        div()
            .occlude()
            .w(PANEL_W)
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .p(spacing(Spacing::Sm, Density::Cozy))
            .bg(p.elevated)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(p.border_input)
            .child(self.render_month_nav(cx))
            .child(self.render_weekday_header())
            .child(self.render_day_grid(cx))
            .child(div().w_full().h(HAIRLINE).bg(p.border_regular))
            .child(self.render_time(cx))
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

fn month_name(month: Month) -> &'static str {
    match month {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    }
}
