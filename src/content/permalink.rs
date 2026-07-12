use std::fmt;
use std::sync::LazyLock;

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
    /// The conventional template applied when a collection sets no `permalink`.
    pub const CONVENTION: &'static str = "/{collection}/{slug}/";

    /// The permalink for an optional, *pre-validated* template string (checked
    /// at config parse), falling back to [`Self::CONVENTION`] when absent.
    pub fn of(template: Option<&str>) -> Self {
        match template.map(Self::parse) {
            Some(Ok(permalink)) => permalink,
            // Templates are validated when the config is parsed, so reaching
            // this arm is a bug: fail loudly in debug, fall back to the
            // convention rather than panicking mid-build in release.
            Some(Err(e)) => {
                debug_assert!(
                    false,
                    "permalink template not validated at config parse: {e}"
                );
                Self::convention()
            }
            None => Self::convention(),
        }
    }

    /// The conventional `/{collection}/{slug}/` permalink — [`Self::CONVENTION`]
    /// parsed once through the same parser as every user template.
    pub fn convention() -> Self {
        static PARSED: LazyLock<Permalink> = LazyLock::new(|| {
            Permalink::parse(Permalink::CONVENTION).expect("the const convention template parses")
        });
        PARSED.clone()
    }

    /// Parse a template string into segments. Unknown placeholders, an
    /// unterminated `{`, and `..` path segments all error.
    pub fn parse(src: &str) -> Result<Self, PermalinkError> {
        // A `..` segment would resolve outside the output directory.
        if src.split('/').any(|segment| segment == "..") {
            return Err(PermalinkError::Traversal);
        }
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
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if !closed {
                    return Err(PermalinkError::Unterminated { name });
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

    /// A rooted, trailing-slashed URL path from already-slugged segments:
    /// `["notes", "rust"]` → `/notes/rust/`, `[]` → `/`. The single joiner for
    /// *generated* (non-template) URLs — taxonomy, pagination, and root pages —
    /// so trailing-slash and separator policy lives in one place instead of a
    /// `format!` at each call site.
    pub fn join(segments: &[&str]) -> String {
        let mut url = String::from("/");
        for segment in segments {
            url.push_str(segment);
            url.push('/');
        }
        url
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
    ("year", |ctx| {
        ctx.date.map(|d| d.year().to_string()).unwrap_or_default()
    }),
    ("month", |ctx| {
        ctx.date
            .map(|d| format!("{:02}", u8::from(d.month())))
            .unwrap_or_default()
    }),
    ("day", |ctx| {
        ctx.date
            .map(|d| format!("{:02}", d.day()))
            .unwrap_or_default()
    }),
    ("order", |ctx| {
        ctx.order.map(|o| o.to_string()).unwrap_or_default()
    }),
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
    /// Context from a page's already-resolved `slug` (frontmatter-else-stem
    /// precedence is decided once, in `Page::load`).
    pub fn from_page(collection: &str, fm: &Frontmatter, slug: &str) -> Self {
        Self {
            slug: slug.to_owned(),
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

    #[error("unterminated `{{{name}` in permalink template")]
    #[diagnostic(
        code(baudelaire::permalink::unterminated),
        help("close the placeholder with `}}`, e.g. `{{{name}}}`")
    )]
    Unterminated { name: String },

    #[error("permalink template must not contain `..` segments")]
    #[diagnostic(
        code(baudelaire::permalink::traversal),
        help("a permalink cannot point outside the output directory")
    )]
    Traversal,
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
        assert!(matches!(
            Permalink::parse("/{bogus}/"),
            Err(PermalinkError::UnknownPlaceholder { .. })
        ));
    }

    #[test]
    fn errors_on_unterminated_placeholder() {
        assert!(matches!(
            Permalink::parse("/posts/{slug"),
            Err(PermalinkError::Unterminated { name }) if name == "slug"
        ));
    }

    #[test]
    fn errors_on_parent_dir_segment() {
        assert!(matches!(
            Permalink::parse("/../{slug}/"),
            Err(PermalinkError::Traversal)
        ));
        // `..` embedded in a longer segment is not a parent-dir component.
        assert!(Permalink::parse("/dots../{slug}/").is_ok());
    }

    #[test]
    fn convention_is_the_parsed_const() {
        let p = Permalink::convention();
        assert_eq!(p.to_string(), Permalink::CONVENTION);
        assert_eq!(p.render(&ctx("hello", "notes")), "/notes/hello/");
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
    fn join_roots_and_trailing_slashes_segments() {
        assert_eq!(Permalink::join(&[]), "/");
        assert_eq!(Permalink::join(&["notes"]), "/notes/");
        assert_eq!(Permalink::join(&["notes", "rust"]), "/notes/rust/");
        assert_eq!(Permalink::join(&["tags", "page", "2"]), "/tags/page/2/");
    }

    #[test]
    fn roundtrips_via_display() {
        let src = "/posts/{year}/{slug}/";
        let p = Permalink::parse(src).unwrap();
        assert_eq!(p.to_string(), src);
    }
}
