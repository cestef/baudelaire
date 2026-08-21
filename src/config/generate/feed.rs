//! `generate { feed { } }`: syndication feeds and their file names.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::config::{BaseUrl, Named, Permalink};
use crate::mime::Mime;

/// Syndication feeds.
#[derive(Debug, Clone, Hash, Table)]
pub struct FeedConfig {
    /// Which feed formats to write, one word each.
    ///
    /// Requires `url`.
    #[key(choices(FeedKind))]
    pub formats: Vec<FeedKind>,

    /// How many of the newest pages a feed carries.
    #[key(count)]
    pub limit: usize,

    /// How much of each page an entry carries: its summary, or its prose as well.
    #[key(choice(Content))]
    pub content: Content,

    /// Also write a feed per taxonomy term.
    ///
    /// Beside that term's listing page (`/tags/rust/rss.xml`). Follows the term
    /// pages, so it needs `listing` on the taxonomy.
    #[key(flag)]
    pub terms: bool,

    /// What each format's file is called, if not the conventional name.
    #[key(nested(FeedNames))]
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
///
/// Each name is a *path* the emitter extends `dist` with, so each is read
/// through [`NodeExt::contained`](crate::config::node::NodeExt::contained) to
/// keep `rss "../../pwned.xml"` inside the project.
#[derive(Debug, Clone, Default, Hash, Table)]
pub struct FeedNames {
    /// The RSS file's name, e.g. `index.xml`. Defaults to `rss.xml`.
    #[key(opt contained)]
    pub rss: Option<String>,

    /// The Atom file's name. Defaults to `atom.xml`.
    #[key(opt contained)]
    pub atom: Option<String>,

    /// The JSON Feed file's name. Defaults to `feed.json`.
    #[key(opt contained)]
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
            Self::Rss => Mime::RSS,
            Self::Atom => Mime::ATOM,
            Self::Json => Mime::JSON_FEED,
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
