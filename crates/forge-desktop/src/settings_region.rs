use std::sync::Arc;

use forge_components::{
    ActivePresentation, ClockStyle, DateDisplay, DateFormatChoice, DateShape, DisplayFormatChoices,
    DisplayFormats, FirstWeekday, ForgePalette, FormatChoice, FormatOption, Region, choice_trigger,
    drive_overlay_focus, dropdown, dropdown_list, dropdown_row, install_display_formats,
    setting_row, setting_rows, tr, ui_names_language,
};
use forge_storage::{SettingsRepo, reserved_keys};
use forge_types::PatternZone;
use gpui::{
    AnyElement, ClickEvent, Context, FocusHandle, Pixels, SharedString, Window, div, prelude::*, px,
};
use time::OffsetDateTime;

use crate::async_bridge::{self, ErrorSink};
use crate::presentation::ActiveDisplayFormats;

const SELECT_WIDTH: Pixels = px(240.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionRow {
    Region,
    DateFormat,
    TimeFormat,
    FirstDay,
}

impl RegionRow {
    const ALL: &[RegionRow] = &[
        RegionRow::Region,
        RegionRow::DateFormat,
        RegionRow::TimeFormat,
        RegionRow::FirstDay,
    ];

    fn title_key(self) -> &'static str {
        match self {
            RegionRow::Region => "settings_region_region",
            RegionRow::DateFormat => "settings_region_date_format",
            RegionRow::TimeFormat => "settings_region_time_format",
            RegionRow::FirstDay => "settings_region_first_day",
        }
    }

    fn id(self) -> &'static str {
        match self {
            RegionRow::Region => "region",
            RegionRow::DateFormat => "date-format",
            RegionRow::TimeFormat => "time-format",
            RegionRow::FirstDay => "first-day",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionPick {
    Region(Region),
    DateFormat(FormatChoice<DateFormatChoice>),
    TimeFormat(FormatChoice<ClockStyle>),
    FirstDay(FormatChoice<FirstWeekday>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionOption {
    pub label: String,
    pub sample: Option<String>,
    pub pick: RegionPick,
}

pub struct SettingsRegionView {
    settings: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    choices: DisplayFormatChoices,
    open: Option<RegionRow>,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
}

impl SettingsRegionView {
    pub fn new(
        settings: Arc<dyn SettingsRepo>,
        rt_handle: tokio::runtime::Handle,
        choices: DisplayFormatChoices,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            settings,
            rt_handle,
            choices,
            open: None,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
        }
    }

    fn toggle_menu(&mut self, row: RegionRow, cx: &mut Context<Self>) {
        self.open = if self.open == Some(row) {
            None
        } else {
            Some(row)
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.open.take().is_some() {
            cx.notify();
        }
    }

    pub fn apply_pick(&mut self, pick: RegionPick) -> Option<(&'static str, &'static str)> {
        let before = self.choices;
        let write = match pick {
            RegionPick::Region(region) => {
                self.choices.region = region;
                (reserved_keys::REGION, region.code())
            }
            RegionPick::DateFormat(choice) => {
                self.choices.date_format = choice;
                (reserved_keys::DATE_FORMAT, choice.code())
            }
            RegionPick::TimeFormat(choice) => {
                self.choices.clock = choice;
                (reserved_keys::TIME_FORMAT, choice.code())
            }
            RegionPick::FirstDay(choice) => {
                self.choices.first_day = choice;
                (reserved_keys::FIRST_DAY_OF_WEEK, choice.code())
            }
        };
        (self.choices != before).then_some(write)
    }

    fn pick(&mut self, pick: RegionPick, cx: &mut Context<Self>) {
        self.open = None;
        if let Some((key, code)) = self.apply_pick(pick) {
            install_display_formats(self.choices.resolve());
            cx.set_global(ActiveDisplayFormats(self.choices));
            let settings = Arc::clone(&self.settings);
            async_bridge::report_failure(
                &self.rt_handle,
                async move { settings.set_string(key, code).await },
                ErrorSink::Toast,
                tr!("settings_region_persist_failed"),
                cx,
            );
            cx.refresh_windows();
        }
        cx.notify();
    }

    fn sample_display(&self, formats: DisplayFormats) -> DateDisplay {
        DateDisplay::new(formats, ui_names_language(), PatternZone::Local)
    }

    fn date_sample(&self, date_format: DateFormatChoice, now: OffsetDateTime) -> String {
        let formats = DisplayFormats {
            date_format,
            ..self.choices.resolve()
        };
        self.sample_display(formats).format(DateShape::Date, now)
    }

    fn time_sample(&self, clock: ClockStyle, now: OffsetDateTime) -> String {
        let formats = DisplayFormats {
            clock,
            ..self.choices.resolve()
        };
        self.sample_display(formats).format(DateShape::Time, now)
    }

    fn option_label(&self, pick: RegionPick, now: OffsetDateTime) -> String {
        let defaults = self.choices.region.defaults();
        match pick {
            RegionPick::Region(region) => tr!(region.name_key()),
            RegionPick::DateFormat(FormatChoice::Fixed(choice)) => self.date_sample(choice, now),
            RegionPick::DateFormat(FormatChoice::RegionDefault) => tr!(
                "settings_region_default",
                value = self.date_sample(defaults.date_format, now)
            ),
            RegionPick::TimeFormat(FormatChoice::Fixed(clock)) => tr!(clock.label_key()),
            RegionPick::TimeFormat(FormatChoice::RegionDefault) => tr!(
                "settings_region_default",
                value = tr!(defaults.clock.label_key())
            ),
            RegionPick::FirstDay(FormatChoice::Fixed(day)) => tr!(day.label_key()),
            RegionPick::FirstDay(FormatChoice::RegionDefault) => tr!(
                "settings_region_default",
                value = tr!(defaults.first_day.label_key())
            ),
        }
    }

    fn option_sample(&self, pick: RegionPick, now: OffsetDateTime) -> Option<String> {
        match pick {
            RegionPick::TimeFormat(choice) => {
                Some(self.time_sample(choice.resolve(self.choices.region.defaults().clock), now))
            }
            _ => None,
        }
    }

    fn current_pick(&self, row: RegionRow) -> RegionPick {
        match row {
            RegionRow::Region => RegionPick::Region(self.choices.region),
            RegionRow::DateFormat => RegionPick::DateFormat(self.choices.date_format),
            RegionRow::TimeFormat => RegionPick::TimeFormat(self.choices.clock),
            RegionRow::FirstDay => RegionPick::FirstDay(self.choices.first_day),
        }
    }

    fn picks(row: RegionRow) -> Vec<RegionPick> {
        match row {
            RegionRow::Region => Region::ALL.into_iter().map(RegionPick::Region).collect(),
            RegionRow::DateFormat => format_picks(RegionPick::DateFormat),
            RegionRow::TimeFormat => format_picks(RegionPick::TimeFormat),
            RegionRow::FirstDay => format_picks(RegionPick::FirstDay),
        }
    }

    pub fn options(&self, row: RegionRow, now: OffsetDateTime) -> Vec<RegionOption> {
        Self::picks(row)
            .into_iter()
            .map(|pick| RegionOption {
                label: self.option_label(pick, now),
                sample: self.option_sample(pick, now),
                pick,
            })
            .collect()
    }

    fn select_control(
        &self,
        row: RegionRow,
        now: OffsetDateTime,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.current_pick(row);
        let trigger = choice_trigger(
            SharedString::from(format!("settings-region-{}-trigger", row.id())),
            self.option_label(current, now).into(),
            true,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_menu(row, cx)),
        );
        let mut field = div()
            .relative()
            .w(SELECT_WIDTH)
            .min_w(SELECT_WIDTH)
            .max_w(SELECT_WIDTH)
            .child(trigger);
        if self.open == Some(row) {
            field = field.child(self.menu(row, current, now, palette, cx));
        }
        field.into_any_element()
    }

    fn menu(
        &self,
        row: RegionRow,
        current: RegionPick,
        now: OffsetDateTime,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut panel = dropdown_list(
            SharedString::from(format!("settings-region-{}-list", row.id())),
            palette,
        );
        for (idx, option) in self.options(row, now).into_iter().enumerate() {
            let pick = option.pick;
            panel = panel.child(
                dropdown_row(
                    (
                        SharedString::from(format!("settings-region-{}", row.id())),
                        idx,
                    ),
                    option.label,
                    palette,
                )
                .suffix(option.sample.map(SharedString::from))
                .current(pick == current)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick(pick, cx))),
            );
        }
        let view = cx.entity();
        dropdown(panel)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }
}

fn format_picks<T: FormatOption>(wrap: fn(FormatChoice<T>) -> RegionPick) -> Vec<RegionPick> {
    std::iter::once(FormatChoice::RegionDefault)
        .chain(T::ALL.iter().copied().map(FormatChoice::Fixed))
        .map(wrap)
        .collect()
}

impl Render for SettingsRegionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.open.is_some(),
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );
        let palette = cx.palette();
        let density = cx.density();
        let now = OffsetDateTime::now_utc();
        let mut rows = setting_rows(&palette);
        for &row in RegionRow::ALL {
            let control = self.select_control(row, now, &palette, cx);
            rows = rows.row(setting_row(
                tr!(row.title_key()),
                None,
                control,
                &palette,
                density,
            ));
        }
        rows.in_card(&palette)
    }
}
