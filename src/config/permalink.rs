use std::fmt;
use std::sync::LazyLock;

use crate::ui::{Code, Text};

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
    ///
    /// `{path}` rather than `{collection}`, so the URL mirrors the content tree
    /// the way every other generator's does: `content/guide/deploy/s3.typ` used
    /// to publish at `/guide/s3/`, losing `deploy/` entirely, and two files with
    /// the same stem in sibling subdirectories were an outright collision.
    ///
    /// For the conventional layout the two are the same string, since a
    /// collection *is* a directory under `content/`. They part company only
    /// where a collection is defined by a glob over a differently-named
    /// directory, and there `{path}` is the honest answer: it says where the
    /// page is.
    pub const CONVENTION: &'static str = "/{path}/{slug}/";

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

    /// The conventional `/{collection}/{slug}/` permalink, [`Self::CONVENTION`]
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
        let mut chars = src.chars();
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
        let raw: String = self.segments.iter().map(|s| s.render(ctx)).collect();
        Self::collapse(&raw)
    }

    /// Drop empty segments from a rendered path, keeping the leading and
    /// trailing slash.
    ///
    /// A placeholder with nothing to render (`{year}` on a dateless page under
    /// `/posts/{year}/{slug}/`) otherwise leaves `/posts//hello/` in
    /// `page.permalink`. Normalizing the *file* path is not enough: the raw
    /// string is what the sitemap, feeds, search index, `llms.txt`,
    /// `baudelaire:pages` and every rewritten `<a href>` emit.
    fn collapse(url: &str) -> String {
        let mut out = String::with_capacity(url.len());
        if url.starts_with('/') {
            out.push('/');
        }
        for (i, segment) in url.split('/').filter(|s| !s.is_empty()).enumerate() {
            if i > 0 {
                out.push('/');
            }
            out.push_str(segment);
        }
        if url.ends_with('/') && !out.ends_with('/') {
            out.push('/');
        }
        out
    }

    /// A rooted, trailing-slashed URL path from already-slugged segments:
    /// `["notes", "rust"]` -> `/notes/rust/`, `[]` -> `/`. The single joiner for
    /// *generated* (non-template) URLs, taxonomy, pagination, and root pages,
    /// so trailing-slash and separator policy lives in one place instead of a
    /// `format!` at each call site.
    ///
    /// An empty segment contributes nothing rather than a bare separator. A
    /// caller passing one is naming "no scope" (the default language's, which
    /// is `""`), and `//search.json` is not that path: a browser reads a
    /// leading `//` as protocol-relative and fetches `http://search.json/`.
    pub fn join(segments: &[&str]) -> String {
        let mut url = String::from("/");
        for segment in segments.iter().filter(|s| !s.is_empty()) {
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
    // Renders several segments at once, which is why it is a `/`-joined string
    // rather than one name: `{path}` is where the page sits, however deep.
    ("path", |ctx| ctx.path.join("/")),
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
    /// The directories the page sits under, relative to the content root, with
    /// a bundle's own directory dropped (it is already the slug). The same
    /// chain the nav tree nests by, so `{path}` and the sidebar cannot
    /// disagree about where a page lives.
    pub path: Vec<String>,
    pub date: Option<time::Date>,
    pub order: Option<i64>,
}

impl PermalinkError {
    /// An unknown `{placeholder}`, its help listing the valid names straight
    /// from [`PLACEHOLDERS`].
    fn unknown(name: &str) -> Self {
        let names: Vec<&str> = PLACEHOLDERS.iter().map(|(n, _)| *n).collect();
        Self::UnknownPlaceholder {
            name: name.to_owned(),
            valid: crate::config::dispatch::Keys::of(&names).help(name, "placeholders"),
        }
    }
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum PermalinkError {
    #[error("unknown permalink placeholder {}", Code(.name))]
    #[diagnostic(code(baudelaire::permalink::unknown_placeholder))]
    UnknownPlaceholder {
        name: String,
        #[help]
        valid: String,
    },

    #[error("unterminated `{{{}` in permalink template", Text(.name))]
    #[diagnostic(
        code(baudelaire::permalink::unterminated),
        help("close the placeholder with `}}`, e.g. `{{{}}}`", Text(.name))
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
            path: vec![col.into()],
            date: time::Date::from_calendar_date(2024, time::Month::January, 15).ok(),
            order: Some(3),
        }
    }

    /// A placeholder with nothing to render collapses its segment away instead
    /// of emitting `/posts//hello/` as the page's canonical URL.
    #[test]
    fn an_unset_placeholder_leaves_no_empty_segment() {
        let dateless = PermalinkCtx {
            slug: "hello".into(),
            collection: "posts".into(),
            path: vec!["posts".into()],
            date: None,
            order: None,
        };
        let p = Permalink::parse("/posts/{year}/{month}/{slug}/").unwrap();
        assert_eq!(p.render(&dateless), "/posts/hello/");
        assert_eq!(p.render(&ctx("hello", "posts")), "/posts/2024/01/hello/");
    }

    /// A template that renders to nothing at all is still the site root.
    #[test]
    fn an_empty_render_is_the_root() {
        let p = Permalink::parse("/{order}/").unwrap();
        let orderless = PermalinkCtx {
            slug: "x".into(),
            collection: "y".into(),
            path: vec!["y".into()],
            date: None,
            order: None,
        };
        assert_eq!(p.render(&orderless), "/");
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

    /// `{path}` is where the page sits, however deep, and renders as several
    /// segments at once.
    #[test]
    fn renders_a_nested_path() {
        let nested = PermalinkCtx {
            slug: "s3".into(),
            collection: "guide".into(),
            path: vec!["guide".into(), "deploy".into()],
            date: None,
            order: None,
        };
        let p = Permalink::parse("/{path}/{slug}/").unwrap();
        assert_eq!(p.render(&nested), "/guide/deploy/s3/");
        // `{collection}` is the collection's name, and still flattens.
        let flat = Permalink::parse("/{collection}/{slug}/").unwrap();
        assert_eq!(flat.render(&nested), "/guide/s3/");
    }

    /// A page directly under the content root has no nesting, and the empty
    /// render collapses rather than leaving `//`.
    #[test]
    fn an_empty_path_leaves_no_stray_separator() {
        let unnested = PermalinkCtx {
            slug: "about".into(),
            collection: "pages".into(),
            path: Vec::new(),
            date: None,
            order: None,
        };
        assert_eq!(
            Permalink::parse("/{path}/{slug}/")
                .unwrap()
                .render(&unnested),
            "/about/"
        );
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
    fn join_roots_and_trailing_slashes_segments() {
        assert_eq!(Permalink::join(&[]), "/");
        assert_eq!(Permalink::join(&["notes"]), "/notes/");
        // The default language's scope is `""`, and a bare `//` is not "the
        // site root": a browser reads it as protocol-relative and fetches
        // `http://search.json/`, so the search client silently found nothing.
        assert_eq!(Permalink::join(&[""]), "/");
        assert_eq!(Permalink::join(&["", "notes"]), "/notes/");
        assert_eq!(Permalink::join(&["notes", ""]), "/notes/");
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
