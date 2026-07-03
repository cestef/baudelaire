use std::fmt;

use crate::content::Frontmatter;

/// A permalink template parsed from a config string like `/posts/{slug}/`.
///
/// Segments are parsed once at config load and rendered per page. No
/// `format!` string interpolation at render time.
#[derive(Debug, Clone)]
pub struct Permalink {
    segments: Vec<Segment>,
}

impl Permalink {
    /// The permalink for an optional template string, falling back to the
    /// conventional `/{collection}/{slug}/` when absent or unparseable.
    pub fn of(template: Option<&str>) -> Self {
        template
            .and_then(|t| Self::parse(t).ok())
            .unwrap_or_else(Self::convention)
    }

    /// The conventional `/{collection}/{slug}/` permalink, built directly so it
    /// can never fail.
    pub fn convention() -> Self {
        Self {
            segments: vec![
                Segment::Literal("/".into()),
                Segment::placeholder("collection"),
                Segment::Literal("/".into()),
                Segment::placeholder("slug"),
                Segment::Literal("/".into()),
            ],
        }
    }

    /// Parse a template string into segments. Unknown placeholders error.
    pub fn parse(src: &str) -> Result<Self, PermalinkError> {
        let mut segments = Vec::new();
        let mut buf = String::new();
        let mut chars = src.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                if !buf.is_empty() {
                    segments.push(Segment::Literal(buf.clone()));
                    buf.clear();
                }
                let mut name = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    name.push(c);
                }
                segments.push(Segment::parse_placeholder(&name)?);
            } else {
                buf.push(c);
            }
        }
        if !buf.is_empty() {
            segments.push(Segment::Literal(buf));
        }
        Ok(Self { segments })
    }

    /// Render to a final URL path.
    pub fn render(&self, ctx: &PermalinkCtx) -> String {
        self.segments.iter().map(|s| s.render(ctx)).collect()
    }
}

impl fmt::Display for Permalink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for s in &self.segments {
            s.fmt(f)?;
        }
        Ok(())
    }
}

/// The permalink placeholders, as `(name, renderer)` pairs. Single source of
/// truth: parsing `{name}`, rendering it, displaying it back, and listing the
/// valid names in errors all read this table.
type Placeholder = (&'static str, fn(&PermalinkCtx) -> String);

const PLACEHOLDERS: &[Placeholder] = &[
    ("slug", |ctx| ctx.slug.clone()),
    ("collection", |ctx| ctx.collection.clone()),
    ("year", |ctx| ctx.date.map(|d| d.year().to_string()).unwrap_or_default()),
    ("month", |ctx| {
        ctx.date
            .map(|d| format!("{:02}", u8::from(d.month())))
            .unwrap_or_default()
    }),
    ("day", |ctx| {
        ctx.date.map(|d| format!("{:02}", d.day())).unwrap_or_default()
    }),
    ("order", |ctx| ctx.order.map(|o| o.to_string()).unwrap_or_default()),
];

/// A single segment of a permalink template.
#[derive(Debug, Clone)]
enum Segment {
    Literal(String),
    /// A `{name}` placeholder and the function that renders it.
    Placeholder(&'static str, fn(&PermalinkCtx) -> String),
}

impl Segment {
    fn parse_placeholder(name: &str) -> Result<Self, PermalinkError> {
        PLACEHOLDERS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(n, render)| Self::Placeholder(n, render))
            .ok_or_else(|| PermalinkError::unknown(name))
    }

    /// Build a known placeholder by name, panicking if absent — for internal
    /// callers passing a name that is guaranteed valid (e.g. [`Permalink::convention`]).
    fn placeholder(name: &'static str) -> Self {
        Self::parse_placeholder(name).expect("known placeholder")
    }

    fn render(&self, ctx: &PermalinkCtx) -> String {
        match self {
            Self::Literal(s) => s.clone(),
            Self::Placeholder(_, render) => render(ctx),
        }
    }
}

impl fmt::Display for Segment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(s) => f.write_str(s),
            Self::Placeholder(name, _) => write!(f, "{{{name}}}"),
        }
    }
}

/// Context for rendering a permalink.
pub struct PermalinkCtx {
    pub slug: String,
    pub collection: String,
    pub date: Option<time::Date>,
    pub order: Option<i64>,
}

impl PermalinkCtx {
    pub fn from_page(collection: &str, fm: &Frontmatter, fallback_slug: &str) -> Self {
        Self {
            slug: fm.slug.clone().unwrap_or_else(|| fallback_slug.to_owned()),
            collection: collection.to_owned(),
            date: fm.date,
            order: fm.order,
        }
    }
}

impl PermalinkError {
    /// An unknown `{placeholder}`, its help listing the valid names straight
    /// from [`PLACEHOLDERS`].
    fn unknown(name: &str) -> Self {
        let valid = PLACEHOLDERS
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
            .join(", ");
        Self::UnknownPlaceholder {
            name: name.to_owned(),
            valid: format!("valid placeholders: {valid}"),
        }
    }
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum PermalinkError {
    #[error("unknown permalink placeholder `{name}`")]
    #[diagnostic(code(baudelaire::permalink::unknown_placeholder))]
    UnknownPlaceholder {
        name: String,
        #[help]
        valid: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(slug: &str, col: &str) -> PermalinkCtx {
        PermalinkCtx {
            slug: slug.into(),
            collection: col.into(),
            date: time::Date::from_calendar_date(2024, time::Month::January, 15).ok(),
            order: Some(3),
        }
    }

    #[test]
    fn parses_literal_only() {
        let p = Permalink::parse("/about/").unwrap();
        assert_eq!(p.render(&ctx("x", "y")), "/about/");
    }

    #[test]
    fn renders_slug() {
        let p = Permalink::parse("/posts/{slug}/").unwrap();
        assert_eq!(p.render(&ctx("hello", "posts")), "/posts/hello/");
    }

    #[test]
    fn renders_collection_and_slug() {
        let p = Permalink::parse("/{collection}/{slug}/").unwrap();
        assert_eq!(p.render(&ctx("hello", "notes")), "/notes/hello/");
    }

    #[test]
    fn renders_date_parts() {
        let p = Permalink::parse("/posts/{year}/{month}/{day}/{slug}/").unwrap();
        assert_eq!(p.render(&ctx("hello", "posts")), "/posts/2024/01/15/hello/");
    }

    #[test]
    fn renders_order() {
        let p = Permalink::parse("/notes/{order}-{slug}/").unwrap();
        assert_eq!(p.render(&ctx("first", "notes")), "/notes/3-first/");
    }

    #[test]
    fn errors_on_unknown_placeholder() {
        assert!(Permalink::parse("/{bogus}/").is_err());
    }

    #[test]
    fn missing_date_renders_empty() {
        let mut c = ctx("hello", "posts");
        c.date = None;
        let p = Permalink::parse("/posts/{year}/{slug}/").unwrap();
        // empty year leaves a blank segment; clean-url pass normalizes later
        assert_eq!(p.render(&c), "/posts//hello/");
    }

    #[test]
    fn roundtrips_via_display() {
        let src = "/posts/{year}/{slug}/";
        let p = Permalink::parse(src).unwrap();
        assert_eq!(p.to_string(), src);
    }
}
