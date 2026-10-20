const MONTHS_IN_YEAR: usize = 12;
const DAYS_IN_WEEK: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NamesLanguage {
    #[default]
    En,
    Uk,
}

impl NamesLanguage {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::En),
            "uk" => Some(Self::Uk),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Uk => "uk",
        }
    }

    fn names(self) -> &'static CalendarNames {
        match self {
            Self::En => &EN,
            Self::Uk => &UK,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameForm {
    Format,
    StandAlone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameWidth {
    Abbreviated,
    Wide,
}

struct NameSet<const N: usize> {
    format_abbreviated: [&'static str; N],
    format_wide: [&'static str; N],
    stand_alone_abbreviated: [&'static str; N],
    stand_alone_wide: [&'static str; N],
}

impl<const N: usize> NameSet<N> {
    fn pick(&self, form: NameForm, width: NameWidth, index: usize) -> &'static str {
        let table = match (form, width) {
            (NameForm::Format, NameWidth::Abbreviated) => &self.format_abbreviated,
            (NameForm::Format, NameWidth::Wide) => &self.format_wide,
            (NameForm::StandAlone, NameWidth::Abbreviated) => &self.stand_alone_abbreviated,
            (NameForm::StandAlone, NameWidth::Wide) => &self.stand_alone_wide,
        };
        table.get(index).copied().unwrap_or_default()
    }
}

struct CalendarNames {
    months: NameSet<MONTHS_IN_YEAR>,
    weekdays_from_monday: NameSet<DAYS_IN_WEEK>,
    before_noon: &'static str,
    after_noon: &'static str,
}

pub(super) fn month_name(
    language: NamesLanguage,
    form: NameForm,
    width: NameWidth,
    month_from_one: u8,
) -> &'static str {
    let index = usize::from(month_from_one).saturating_sub(1);
    language.names().months.pick(form, width, index)
}

pub(super) fn weekday_name(
    language: NamesLanguage,
    form: NameForm,
    width: NameWidth,
    days_from_monday: u8,
) -> &'static str {
    language
        .names()
        .weekdays_from_monday
        .pick(form, width, usize::from(days_from_monday))
}

pub(super) fn day_period(language: NamesLanguage, is_after_noon: bool) -> &'static str {
    let names = language.names();
    if is_after_noon {
        names.after_noon
    } else {
        names.before_noon
    }
}

const EN_MONTHS_ABBREVIATED: [&str; MONTHS_IN_YEAR] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const EN_MONTHS_WIDE: [&str; MONTHS_IN_YEAR] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const EN_WEEKDAYS_ABBREVIATED: [&str; DAYS_IN_WEEK] =
    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const EN_WEEKDAYS_WIDE: [&str; DAYS_IN_WEEK] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

static EN: CalendarNames = CalendarNames {
    months: NameSet {
        format_abbreviated: EN_MONTHS_ABBREVIATED,
        format_wide: EN_MONTHS_WIDE,
        stand_alone_abbreviated: EN_MONTHS_ABBREVIATED,
        stand_alone_wide: EN_MONTHS_WIDE,
    },
    weekdays_from_monday: NameSet {
        format_abbreviated: EN_WEEKDAYS_ABBREVIATED,
        format_wide: EN_WEEKDAYS_WIDE,
        stand_alone_abbreviated: EN_WEEKDAYS_ABBREVIATED,
        stand_alone_wide: EN_WEEKDAYS_WIDE,
    },
    before_noon: "AM",
    after_noon: "PM",
};

const UK_MONTHS_ABBREVIATED: [&str; MONTHS_IN_YEAR] = [
    "січ.",
    "лют.",
    "бер.",
    "квіт.",
    "трав.",
    "черв.",
    "лип.",
    "серп.",
    "вер.",
    "жовт.",
    "лист.",
    "груд.",
];
const UK_WEEKDAYS_ABBREVIATED: [&str; DAYS_IN_WEEK] = ["пн", "вт", "ср", "чт", "пт", "сб", "нд"];
const UK_WEEKDAYS_WIDE: [&str; DAYS_IN_WEEK] = [
    "понеділок",
    "вівторок",
    "середа",
    "четвер",
    "пʼятниця",
    "субота",
    "неділя",
];

static UK: CalendarNames = CalendarNames {
    months: NameSet {
        format_abbreviated: UK_MONTHS_ABBREVIATED,
        format_wide: [
            "січня",
            "лютого",
            "березня",
            "квітня",
            "травня",
            "червня",
            "липня",
            "серпня",
            "вересня",
            "жовтня",
            "листопада",
            "грудня",
        ],
        stand_alone_abbreviated: UK_MONTHS_ABBREVIATED,
        stand_alone_wide: [
            "січень",
            "лютий",
            "березень",
            "квітень",
            "травень",
            "червень",
            "липень",
            "серпень",
            "вересень",
            "жовтень",
            "листопад",
            "грудень",
        ],
    },
    weekdays_from_monday: NameSet {
        format_abbreviated: UK_WEEKDAYS_ABBREVIATED,
        format_wide: UK_WEEKDAYS_WIDE,
        stand_alone_abbreviated: UK_WEEKDAYS_ABBREVIATED,
        stand_alone_wide: UK_WEEKDAYS_WIDE,
    },
    before_noon: "дп",
    after_noon: "пп",
};
