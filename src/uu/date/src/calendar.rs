// This file is part of the uutils coreutils package.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use icu_calendar::{Date as IcuDate, Iso, cal};
use icu_datetime::{DateTimeFormatter, fieldsets};
use icu_locale::{Locale, langid};
use icu_time::{DateTime as IcuDateTime, Time as IcuTime};
use jiff::Zoned;
use jiff_icu::ConvertFrom;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarType {
    Gregorian,
    Ethiopian,
    Persian,
    Thai,
}

impl CalendarType {
    pub fn from_locale() -> Self {
        let loc = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_TIME"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        if loc.starts_with("am_ET") {
            Self::Ethiopian
        } else if loc.starts_with("fa_IR") {
            Self::Persian
        } else if loc.starts_with("th_TH") {
            Self::Thai
        } else {
            Self::Gregorian
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Self::Gregorian => "",
            Self::Ethiopian => "-u-ca-ethiopic",
            Self::Persian => "-u-ca-persian",
            Self::Thai => "-u-ca-buddhist",
        }
    }
}

pub struct CalendarDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

fn loc_base(s: &str) -> String {
    s.split('.').next().unwrap_or("en_US").replace('_', "-")
}
fn loc_cal(s: &str, c: CalendarType) -> Locale {
    format!("{}{}", loc_base(s), c.ext())
        .parse()
        .unwrap_or_else(|_| langid!("en-US").into())
}
fn loc_only(s: &str) -> Locale {
    loc_base(s)
        .parse()
        .unwrap_or_else(|_| langid!("en-US").into())
}
fn icu_t(d: &Zoned) -> IcuTime {
    IcuTime::try_new(d.hour() as u8, d.minute() as u8, d.second() as u8, 0).unwrap()
}
fn icu_d(d: &Zoned) -> IcuDate<Iso> {
    IcuDate::<Iso>::convert_from(d.date())
}

macro_rules! cal_fmt {
    ($f:expr, $d:expr, $c:expr) => {
        match $c {
            CalendarType::Gregorian => $f.format(&$d).to_string(),
            CalendarType::Ethiopian => $f
                .format(&$d.to_calendar(cal::Ethiopian::new()))
                .to_string(),
            CalendarType::Persian => $f.format(&$d.to_calendar(cal::Persian::new())).to_string(),
            CalendarType::Thai => $f.format(&$d.to_calendar(cal::Buddhist)).to_string(),
        }
    };
}

macro_rules! cal_dt_fmt {
    ($f:expr, $d:expr, $t:expr, $c:expr) => {
        match $c {
            CalendarType::Gregorian => $f.format(&IcuDateTime { date: $d, time: $t }).to_string(),
            CalendarType::Ethiopian => $f
                .format(&IcuDateTime {
                    date: $d.to_calendar(cal::Ethiopian::new()),
                    time: $t,
                })
                .to_string(),
            CalendarType::Persian => $f
                .format(&IcuDateTime {
                    date: $d.to_calendar(cal::Persian::new()),
                    time: $t,
                })
                .to_string(),
            CalendarType::Thai => $f
                .format(&IcuDateTime {
                    date: $d.to_calendar(cal::Buddhist),
                    time: $t,
                })
                .to_string(),
        }
    };
}

pub fn convert_to_calendar(date: &Zoned, c: CalendarType) -> CalendarDate {
    let iso = icu_d(date);
    macro_rules! cd {
        ($d:expr) => {
            CalendarDate {
                year: $d.year().era_year_or_related_iso(),
                month: $d.month().ordinal as u8,
                day: $d.day_of_month().0 as u8,
            }
        };
    }
    match c {
        CalendarType::Gregorian => CalendarDate {
            year: i32::from(date.year()),
            month: date.month() as u8,
            day: date.day() as u8,
        },
        CalendarType::Ethiopian => cd!(iso.to_calendar(cal::Ethiopian::new())),
        CalendarType::Persian => cd!(iso.to_calendar(cal::Persian::new())),
        CalendarType::Thai => cd!(iso.to_calendar(cal::Buddhist)),
    }
}

fn month(d: &Zoned, l: &str, c: CalendarType, ab: bool) -> String {
    let f = if ab {
        fieldsets::M::medium()
    } else {
        fieldsets::M::long()
    };
    let fmt = DateTimeFormatter::try_new(loc_cal(l, c).into(), f)
        .unwrap_or_else(|_| DateTimeFormatter::try_new(langid!("en-US").into(), f).unwrap());
    cal_fmt!(fmt, icu_d(d), c)
}

fn weekday(d: &Zoned, l: &str, c: CalendarType, ab: bool) -> String {
    let f = if ab {
        fieldsets::E::short()
    } else {
        fieldsets::E::long()
    };
    let fmt = DateTimeFormatter::try_new(loc_cal(l, c).into(), f)
        .unwrap_or_else(|_| DateTimeFormatter::try_new(langid!("en-US").into(), f).unwrap());
    cal_fmt!(fmt, icu_d(d), c)
}

fn time(d: &Zoned, l: &str) -> String {
    let fmt = DateTimeFormatter::try_new(loc_only(l).into(), fieldsets::T::medium())
        .unwrap_or_else(|_| {
            DateTimeFormatter::try_new(langid!("en-US").into(), fieldsets::T::medium()).unwrap()
        });
    fmt.format(&icu_t(d)).to_string()
}

fn datetime(d: &Zoned, l: &str, c: CalendarType) -> String {
    let fmt = DateTimeFormatter::try_new(loc_cal(l, c).into(), fieldsets::YMDET::medium())
        .unwrap_or_else(|_| {
            DateTimeFormatter::try_new(langid!("en-US").into(), fieldsets::YMDET::medium()).unwrap()
        });
    cal_dt_fmt!(fmt, icu_d(d), icu_t(d), c)
}

fn date_short(d: &Zoned, l: &str, c: CalendarType) -> String {
    let fmt = DateTimeFormatter::try_new(loc_cal(l, c).into(), fieldsets::YMD::short())
        .unwrap_or_else(|_| {
            DateTimeFormatter::try_new(langid!("en-US").into(), fieldsets::YMD::short()).unwrap()
        });
    cal_fmt!(fmt, icu_d(d), c)
}

fn day_period(d: &Zoned, l: &str) -> String {
    let fmt = DateTimeFormatter::try_new(loc_only(l).into(), fieldsets::T::short()).unwrap_or_else(
        |_| DateTimeFormatter::try_new(langid!("en-US").into(), fieldsets::T::short()).unwrap(),
    );
    fmt.format(&icu_t(d))
        .to_string()
        .split_whitespace()
        .last()
        .filter(|w| w.chars().all(|c| c.is_alphabetic()))
        .unwrap_or(if d.hour() < 12 { "AM" } else { "PM" })
        .to_string()
}

pub fn format_with_calendar(
    date: &Zoned,
    format: &str,
    c: CalendarType,
    config: &jiff::fmt::strtime::Config<jiff::fmt::strtime::PosixCustom>,
) -> Result<String, jiff::Error> {
    use jiff::fmt::strtime::BrokenDownTime;
    let cd = convert_to_calendar(date, c);
    let l = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_TIME"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| String::from("en_US"));
    let mut out = String::with_capacity(format.len() + 32);
    let mut it = format.chars().peekable();

    while let Some(ch) = it.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        let m = matches!(it.peek(), Some('-' | '_' | '0')).then(|| it.next().unwrap());
        let Some(&s) = it.peek() else {
            out.push('%');
            if let Some(x) = m {
                out.push(x);
            }
            continue;
        };
        match s {
            'Y' => {
                it.next();
                out.push_str(&format!("{:04}", cd.year));
            }
            'y' => {
                it.next();
                out.push_str(&format!("{:02}", (i32::from(date.year()) % 100).abs()));
            }
            'm' => {
                it.next();
                out.push_str(&if m == Some('-') {
                    format!("{}", cd.month)
                } else {
                    format!("{:02}", cd.month)
                });
            }
            'd' => {
                it.next();
                out.push_str(&if m == Some('-') {
                    format!("{}", cd.day)
                } else {
                    format!("{:02}", cd.day)
                });
            }
            'e' => {
                it.next();
                out.push_str(&format!("{:2}", cd.day));
            }
            'B' => {
                it.next();
                out.push_str(&month(date, &l, c, false));
            }
            'b' | 'h' => {
                it.next();
                out.push_str(&month(date, &l, c, true));
            }
            'A' => {
                it.next();
                out.push_str(&weekday(date, &l, c, false));
            }
            'a' => {
                it.next();
                out.push_str(&weekday(date, &l, c, true));
            }
            'p' => {
                it.next();
                out.push_str(&day_period(date, &l).to_uppercase());
            }
            'P' => {
                it.next();
                out.push_str(&day_period(date, &l).to_lowercase());
            }
            'c' => {
                it.next();
                out.push_str(&datetime(date, &l, c));
            }
            'x' => {
                it.next();
                out.push_str(&date_short(date, &l, c));
            }
            'X' | 'r' => {
                it.next();
                out.push_str(&time(date, &l));
            }
            'F' => {
                it.next();
                out.push_str(&format!("{:04}-{:02}-{:02}", cd.year, cd.month, cd.day));
            }
            'D' => {
                it.next();
                out.push_str(&format!(
                    "{:02}/{:02}/{:02}",
                    cd.month,
                    cd.day,
                    (i32::from(date.year()) % 100).abs()
                ));
            }
            _ => {
                out.push('%');
                if let Some(x) = m {
                    out.push(x);
                }
            }
        }
    }
    BrokenDownTime::from(date).to_string_with_config(config, &out)
}
