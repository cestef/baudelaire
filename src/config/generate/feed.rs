//! `generate { feed { } }`: syndication feeds and their file names.

use crate::config::dispatch::Kind::{Block as Nested, Choice, Choices, Flag, Number, Path};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{BaseUrl, Named, Permalink};

/// Syndication feeds.
#[derive(Debug, Clone, Hash)]
pub struct FeedConfig {
    /// Formats to emit (requires `url`).
    pub formats: Vec<FeedKind>,
    /// Maximum items in a feed.
    pub limit: usize,
    pub content: Content,
    /// Also emit a feed per taxonomy term, beside that term's listing page
    /// (`/tags/rust/rss.xml`). Follows the term pages, so it needs `listing` on
    /// the taxonomy.
    pub terms: bool,
    /// What each format's file is called, when the conventional name is not the
    /// one a site already publishes under.
    pub names: FeedNames,
}

/// How much of a page a feed entry carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Content {
    /// The page's `description` alone.
    #[default]
    Summary,
    /// Its rendered prose as well, taken from `html { region }`, so a feed
    /// never carries the site chrome around it.
    Full,
}

impl Named for Content {
    const NAMES: &'static [(&'static str, Self)] =
        &[("summary", Self::Summary), ("full", Self::Full)];
}

/// Per-format file name overrides for [`FeedConfig`].
#[derive(Debug, Clone, Default, Hash)]
pub struct FeedNames {
    pub rss: Option<String>,
    pub atom: Option<String>,
    pub json: Option<String>,
}

impl FeedConfig {
    /// Whether an entry carries the page's prose as well as its summary; the
    /// render pass and the feed emitter both ask it and must agree.
    pub fn full(&self) -> bool {
        self.content == Content::Full
    }

    /// This format's file name: the configured override, else the conventional
    /// one.
    pub fn file(&self, kind: FeedKind) -> &str {
        let named = match kind {
            FeedKind::Rss => &self.names.rss,
            FeedKind::Atom => &self.names.atom,
            FeedKind::Json => &self.names.json,
        };
        named.as_deref().unwrap_or_else(|| kind.file())
    }

    /// This feed's absolute URL under `base`, for a language `scope` (empty for
    /// the default language). The file name is appended to the scope's
    /// directory URL rather than joined as a path segment, which would give it
    /// a trailing slash.
    pub fn url(&self, kind: FeedKind, base: &BaseUrl, scope: &str) -> String {
        format!(
            "{}{}",
            base.join(Permalink::join(&[scope])),
            self.file(kind)
        )
    }
}

/// A syndication feed format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeedKind {
    Rss,
    Atom,
    Json,
}

impl Named for FeedKind {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("rss", Self::Rss),
        ("atom", Self::Atom),
        ("json", Self::Json),
    ];
}

impl FeedKind {
    /// The conventional output file name for this format, which
    /// [`FeedConfig::file`] overrides.
    pub fn file(self) -> &'static str {
        match self {
            Self::Rss => "rss.xml",
            Self::Atom => "atom.xml",
            Self::Json => "feed.json",
        }
    }

    /// The media type a `<link rel="alternate">` announces this format under.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Rss => "application/rss+xml",
            Self::Atom => "application/atom+xml",
            Self::Json => "application/feed+json",
        }
    }
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self {
            formats: Vec::new(),
            limit: 20,
            content: Content::default(),
            terms: false,
            names: crate::config::FeedNames::default(),
        }
    }
}

impl Section for FeedConfig {
    const RULES: Block<Self> = Block(&[
        (
            "formats",
            Choices(FeedKind::names),
            "Which feed formats to write, one word each.",
            |c, n, t| {
                c.formats = n.mapped::<FeedKind>(t)?;
                Ok(())
            },
        ),
        (
            "limit",
            Number,
            "How many of the newest pages a feed carries.",
            |c, n, t| {
                c.limit = n.count(t, 0)?;
                Ok(())
            },
        ),
        (
            "content",
            Choice(Content::names),
            "How much of each page an entry carries: its summary, or its prose as well.",
            |c, n, t| {
                c.content = n.arg(t, 0)?.one::<Content>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "terms",
            Flag,
            "Also write a feed per taxonomy term.",
            |c, n, t| {
                c.terms = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "names",
            Nested(FeedNames::rows),
            "What each format's file is called, if not the conventional name.",
            |c, n, t| c.names.fill(n, t),
        ),
    ]);
}

/// The `feed { names { .. } }` section: one key per format, each naming the
/// file that format is written to and advertised under. Each name is a *path*
/// the emitter extends `dist` with, so each is read through
/// [`NodeExt::contained`] to keep `rss "../../pwned.xml"` inside the project.
impl Section for FeedNames {
    const RULES: Block<Self> = Block(&[
        (
            "rss",
            Path,
            "The RSS file's name, e.g. `index.xml`. Defaults to `rss.xml`.",
            |c, n, t| {
                c.rss = Some(n.contained(t)?);
                Ok(())
            },
        ),
        (
            "atom",
            Path,
            "The Atom file's name. Defaults to `atom.xml`.",
            |c, n, t| {
                c.atom = Some(n.contained(t)?);
                Ok(())
            },
        ),
        (
            "json",
            Path,
            "The JSON Feed file's name. Defaults to `feed.json`.",
            |c, n, t| {
                c.json = Some(n.contained(t)?);
                Ok(())
            },
        ),
    ]);
}

#[cfg(test)]
mod tests {
    use super::{FeedNames, Named as _};
    use crate::config::FeedKind;
    use crate::config::dispatch::Section;

    /// Nothing forces a new format to get a `FeedNames` key, so one could
    /// exist, be written, and be impossible to rename.
    #[test]
    fn every_format_can_be_renamed() {
        for (name, _) in FeedKind::NAMES {
            assert!(
                FeedNames::RULES.0.iter().any(|(key, ..)| key == name),
                "`generate {{ feed {{ names }} }}` has no `{name}` key, so that format's file cannot be renamed"
            );
        }
    }
}
