//! Shared date formatting: the machine-readable day, and the one a reader sees.

use std::fmt;

use crate::content::Strings;

/// A date as an ISO-8601 day (`2026-07-15`), the single date rendering used by
/// listings, the JS `baudelaire:pages`/`baudelaire:feed` modules, and (with a
/// midnight-UTC suffix) publish timestamps.
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

/// The inverse, so the rendering and the reading of a day are one type. A
/// markdown page writes `date "2026-07-15"` in KDL, which has no date literal;
/// a typst page may write the same string rather than `datetime(..)`.
///
/// Strict on purpose: `YYYY-MM-DD`, the form [`Iso`] emits, optionally followed
/// by a time of day. Anything looser would make a plain string that merely
/// resembles a date silently become one.
///
/// The time is validated and then dropped, because a page is dated to a day:
/// this is what a typst page's `datetime(..)` has always done, and reading it
/// only there made one rule differ by dialect. A pasted Hugo or Zola post
/// writes `date = 2024-01-01T10:00:00Z`, which every doc comment here promised
/// would parse and which failed the build instead. The literal day is taken,
/// offset and all, so a timestamp late enough to fall on the next day in UTC is
/// still the day its author wrote.
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
    /// Whether `text` is a time of day, with an optional fraction and an
    /// optional zone: `10:00`, `10:00:00`, `10:00:00.5`, `10:00:00Z`,
    /// `10:00:00+02:00`.
    ///
    /// Shape only, since the value is dropped: what it has to rule out is prose
    /// that happens to follow a date (`2026-01-01 was a good day`), which is the
    /// case the strictness above exists for.
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
/// ISO-8601 is the right answer for a machine (a feed, a sitemap, a `datetime`
/// attribute) and the wrong one for a reader, which is what every listing showed
/// them. Typst cannot fix it in a template either: its own `datetime.display`
/// knows English month names only.
///
/// Both halves come from the per-language `strings` table, so a language
/// declares its own without any locale database: `months` names the twelve, and
/// `date` is the pattern they slot into.
pub struct Localized<'a> {
    date: time::Date,
    strings: &'a Strings<'a>,
}

/// One `{name}` a date pattern accepts, and what fills it. Mirrors the
/// permalink placeholders: one table drives substitution and the documented
/// list alike.
type Placeholder = (&'static str, fn(&Localized) -> String);

impl<'a> Localized<'a> {
    /// The pattern's placeholders, as `(name, renderer)`. The single source of
    /// truth: substitution and the documented list both read it.
    const PLACEHOLDERS: &'static [Placeholder] = &[
        ("month", |d| d.month()),
        ("year", |d| d.date.year().to_string()),
        // Zero-padded before the bare day, so `{day}` inside `{day2}` cannot
        // match first and leave a stray `2`.
        ("day2", |d| format!("{:02}", d.date.day())),
        ("day", |d| d.date.day().to_string()),
    ];

    pub fn new(date: time::Date, strings: &'a Strings<'a>) -> Self {
        Self { date, strings }
    }

    /// This date's month name, from the language's `months` list. A list that
    /// is absent or the wrong length falls back to the English name, which
    /// beats printing a number where a word belongs.
    fn month(&self) -> String {
        let number = u8::from(self.date.month()) as usize;
        self.strings
            .list("months")
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

    /// The built-in default, for a site that declares nothing.
    #[test]
    fn an_undeclared_language_reads_english() {
        let config = Config::default();
        let strings = Strings::new(&config, "en");
        let shown = Localized::new(date(2026, 7, 30), &strings).to_string();
        assert_eq!(shown, "July 30, 2026");
    }

    /// A language names its own months and orders them its own way, with no
    /// locale database in the binary.
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

    /// `{day}` sits inside `{day2}`, so the padded form has to substitute
    /// first or a `{day2}` would render as `30` followed by a stray `2`.
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

    /// A pasted Hugo or Zola post writes its date with a time of day, in every
    /// dialect that has a date literal and in the strings the others write. It
    /// was refused outright, while a typst page's `datetime(..)` had always
    /// been accepted and truncated: one rule that differed by dialect, and
    /// three doc comments promising the paste would work.
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
            // The literal day, offset and all: a timestamp late enough to fall
            // on the next day in UTC is still the day its author wrote.
            assert_eq!(parsed.to_string(), "2024-01-01", "{text}");
        }
    }

    /// And the strictness the truncation must not cost: a string that merely
    /// starts with something date-shaped is still not a date.
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
