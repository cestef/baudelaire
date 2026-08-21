//! Shared date formatting: the machine-readable day, and the one a reader sees.

use std::fmt;

use crate::content::Strings;

/// A date as an ISO-8601 day (`2026-07-15`), the single machine-readable date
/// rendering.
pub struct Iso(pub time::Date);

impl fmt::Display for Iso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}",
            self.0.year(),
            u8::from(self.0.month()),
            self.0.day()
        )
    }
}

/// Strict on purpose: `YYYY-MM-DD`, the form [`Iso`] emits, optionally followed
/// by a time of day, or a string that merely resembles a date would become one.
/// The time is validated and dropped, taking the literal day offset and all.
impl std::str::FromStr for Iso {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (text, time) = match text.split_once(['T', ' ']) {
            Some((day, time)) => (day, Some(time)),
            None => (text, None),
        };
        if time.is_some_and(|time| !Self::is_time(time)) {
            return Err(());
        }
        let [year, month, day] =
            <[&str; 3]>::try_from(text.split('-').collect::<Vec<_>>()).map_err(|_| ())?;
        if (year.len(), month.len(), day.len()) != (4, 2, 2) {
            return Err(());
        }
        let month = month.parse::<u8>().map_err(|_| ())?;
        time::Date::from_calendar_date(
            year.parse().map_err(|_| ())?,
            time::Month::try_from(month).map_err(|_| ())?,
            day.parse().map_err(|_| ())?,
        )
        .map(Self)
        .map_err(|_| ())
    }
}

impl Iso {
    /// Whether `text` is shaped like a time of day, with an optional fraction
    /// and an optional zone: `10:00`, `10:00:00.5`, `10:00:00+02:00`.
    fn is_time(text: &str) -> bool {
        let mut chars = text.chars();
        let hours = matches!((chars.next(), chars.next(), chars.next()),
            (Some(a), Some(b), Some(':')) if a.is_ascii_digit() && b.is_ascii_digit());
        hours && chars.all(|c| c.is_ascii_digit() || matches!(c, ':' | '.' | '+' | '-' | 'Z' | 'z'))
    }
}

/// A date written the way its language writes one: `30 juillet 2026` beside
/// `July 30, 2026`.
///
/// Both halves come from the per-language `strings` table, `months` naming the
/// twelve and `date` the pattern they slot into.
pub struct Localized<'a> {
    date: time::Date,
    strings: &'a Strings<'a>,
}

/// One `{name}` a date pattern accepts, and what fills it.
type Placeholder = (&'static str, fn(&Localized) -> String);

impl<'a> Localized<'a> {
    /// The pattern's placeholders, as `(name, renderer)`, a padded form always
    /// before its bare one so `{day}` cannot match inside `{day2}` and leave a
    /// stray `2`.
    const PLACEHOLDERS: &'static [Placeholder] = &[
        ("month", |d| d.month()),
        ("year", |d| d.date.year().to_string()),
        ("day2", |d| format!("{:02}", d.date.day())),
        ("day", |d| d.date.day().to_string()),
    ];

    /// How many names a `months` list has to carry to be one: a shorter list
    /// would name some months and leave the rest in English.
    const MONTHS: usize = 12;

    pub fn new(date: time::Date, strings: &'a Strings<'a>) -> Self {
        Self { date, strings }
    }

    /// This date's month name from the language's `months` list, falling back
    /// to the English name when that list is absent or the wrong length.
    fn month(&self) -> String {
        let number = u8::from(self.date.month()) as usize;
        self.strings
            .list("months")
            .filter(|months| months.len() == Self::MONTHS)
            .and_then(|months| months.get(number - 1).cloned())
            .unwrap_or_else(|| self.date.month().to_string())
    }
}

impl fmt::Display for Localized<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = self.strings.get("date").to_owned();
        for (name, render) in Self::PLACEHOLDERS {
            let placeholder = format!("{{{name}}}");
            if out.contains(&placeholder) {
                out = out.replace(&placeholder, &render(self));
            }
        }
        f.write_str(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::{Iso, Localized};
    use crate::config::Config;
    use crate::content::Strings;

    fn date(year: i32, month: u8, day: u8) -> time::Date {
        time::Date::from_calendar_date(year, time::Month::try_from(month).unwrap(), day).unwrap()
    }

    #[test]
    fn iso_is_zero_padded() {
        assert_eq!(Iso(date(2026, 7, 5)).to_string(), "2026-07-05");
    }

    #[test]
    fn an_undeclared_language_reads_english() {
        let config = Config::default();
        let strings = Strings::new(&config, "en");
        let shown = Localized::new(date(2026, 7, 30), &strings).to_string();
        assert_eq!(shown, "July 30, 2026");
    }

    #[test]
    fn a_language_declares_its_own_months_and_order() {
        let config = Config::parse(
            r#"
            lang "en"
            languages {
              fr {
                strings {
                  date "{day} {month} {year}"
                  months "janvier" "février" "mars" "avril" "mai" "juin" \
                         "juillet" "août" "septembre" "octobre" "novembre" "décembre"
                }
              }
            }
            "#,
        )
        .expect("should parse");
        let strings = Strings::new(&config, "fr");
        let shown = Localized::new(date(2026, 7, 30), &strings).to_string();
        assert_eq!(shown, "30 juillet 2026");
    }

    #[test]
    fn the_padded_day_is_not_eaten_by_the_bare_one() {
        let config = Config::parse(
            r#"
            lang "en"
            languages { en { strings { date "{year}/{day2}" } } }
            "#,
        )
        .expect("should parse");
        let strings = Strings::new(&config, "en");
        assert_eq!(
            Localized::new(date(2026, 7, 5), &strings).to_string(),
            "2026/05"
        );
    }

    #[test]
    fn a_day_may_carry_a_time_of_day() {
        for text in [
            "2024-01-01",
            "2024-01-01T10:00:00Z",
            "2024-01-01T10:00:00",
            "2024-01-01 10:00:00",
            "2024-01-01T10:00",
            "2024-01-01T10:00:00.5",
            "2024-01-01T23:00:00-05:00",
        ] {
            let parsed: Iso = text.parse().unwrap_or_else(|()| panic!("{text}"));
            assert_eq!(parsed.to_string(), "2024-01-01", "{text}");
        }
    }

    #[test]
    fn prose_after_a_day_is_not_a_time() {
        for text in [
            "2024-01-01 was a good day",
            "2024-01-01Tomorrow",
            "2024-01-01 ",
            "2024-1-01",
            "not a date",
        ] {
            assert!(text.parse::<Iso>().is_err(), "{text}");
        }
    }
}
