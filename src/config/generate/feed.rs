//! `generate { feed { } }`: syndication feeds and their file names.

use crate::config::dispatch::Kind::{Block as Nested, Choice, Flag, Number, Path};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{BaseUrl, Named, Permalink};

/// Syndication feeds.
#[derive(Debug, Clone, Hash)]
pub struct FeedConfig {
    /// Formats to emit (requires `url`).
    pub formats: Vec<FeedKind>,
    /// Maximum items in a feed.
    pub limit: usize,
    /// Also emit a feed per taxonomy term, beside that term's listing page
    /// (`/tags/rust/rss.xml`), so a reader can follow one tag rather than the
    /// whole site. Follows the term pages, so it needs `listing` on the
    /// taxonomy.
    pub terms: bool,
    /// What each format's file is called, when the conventional name is not the
    /// one a site already publishes under. A moved feed is the one move a
    /// redirect stub cannot rescue, since a reader fetches the file and never
    /// renders the meta refresh.
    pub names: FeedNames,
}

/// Per-format file name overrides for [`FeedConfig`].
#[derive(Debug, Clone, Default, Hash)]
pub struct FeedNames {
    pub rss: Option<String>,
    pub atom: Option<String>,
    pub json: Option<String>,
}

impl FeedConfig {
    /// This format's file name: the configured override, else the conventional
    /// one.
    ///
    /// The single answer, because a feed names its own file in three places and
    /// an aggregator would notice them disagreeing: the file the build writes,
    /// the `<id>`/`feed_url` inside it, and every page's autodiscovery tag.
    pub fn file(&self, kind: FeedKind) -> &str {
        let named = match kind {
            FeedKind::Rss => &self.names.rss,
            FeedKind::Atom => &self.names.atom,
            FeedKind::Json => &self.names.json,
        };
        named.as_deref().unwrap_or_else(|| kind.file())
    }

    /// This feed's absolute URL under `base`, for a language `scope` (empty for
    /// the default language).
    ///
    /// The file name is appended to the scope's directory URL rather than
    /// joined as a path segment, which would give it a trailing slash.
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

    /// The media type a `<link rel="alternate">` announces this format under,
    /// and how a reader tells the three apart when a page advertises several.
    /// Beside [`file`](Self::file) because a format's name and its type are the
    /// same fact, and an autodiscovery tag needs both.
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
            // opt-in like search: no feed until a format is named
            formats: Vec::new(),
            limit: 20,
            // off by default: one more file per term per format multiplies the
            // output of a heavily tagged site
            terms: false,
            // each format's conventional file name until a site says otherwise
            names: crate::config::FeedNames::default(),
        }
    }
}

impl Section for FeedConfig {
    const RULES: Block<Self> = Block(&[
        (
            "formats",
            Choice(FeedKind::names),
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
/// file that format is written to and advertised under. A site arriving from a
/// generator that named them differently keeps its subscribers by stating the
/// old names here.
///
/// Each name is a *path*, and the emitter extends `dist` with it, so each is
/// read through [`NodeExt::contained`] like every other generated file name
/// (`navigation { standalone { file } }` is the same rule): `rss
/// "../../pwned.xml"` used to write two directories above the project root and
/// report a green build.
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
