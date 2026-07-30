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
}
