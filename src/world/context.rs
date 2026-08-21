//! Build metadata: what a page can learn about the build that produced it,
//! injected at `sys.inputs.baudelaire` and mirrored into the `site` modules.

use time::OffsetDateTime;

use crate::codegen;
use crate::config::Config;
use crate::git::{Head, Repo};

/// How the site is being produced, exposed as `sys.inputs.baudelaire.mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Build,
    Serve,
    Check,
}

impl Mode {
    /// Where this mode keeps its incremental state, under the configured cache
    /// directory.
    ///
    /// `check` keeps its own: it renders without the asset pipeline, so its
    /// HTML is not the HTML a build writes, and an entry of one read by the
    /// other would serve markup whose asset references were never resolved.
    /// `build` and `serve` share, and must: switching between them would
    /// otherwise be a whole-site miss every time.
    pub fn cache(self, dir: &std::path::Path) -> std::path::PathBuf {
        match self {
            Self::Build | Self::Serve => dir.to_path_buf(),
            Self::Check => dir.join(Self::Check.as_str()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Serve => "serve",
            Self::Check => "check",
        }
    }
}

/// Build metadata exposed to pages via `sys.inputs.baudelaire` (version, build
/// date, mode, active profile, git state, and a mirror of site identity).
///
/// [`crate::graph::Analyzer`] tracks each read per page, so a commit or a new
/// day invalidates only the pages that display the value that moved.
#[derive(Debug, Clone)]
pub struct BuildContext {
    version: &'static str,
    date: String,
    mode: Mode,
    profile: Option<String>,
    git: Option<Head>,
    site: SiteInfo,
    /// The `client { }` constants, at `sys.inputs.baudelaire.client`.
    client: codegen::Value,
}

/// A mirror of site identity, so layouts can read it via `sys.inputs` without
/// per-page frontmatter plumbing.
#[derive(Debug, Clone)]
struct SiteInfo {
    title: Option<String>,
    url: Option<String>,
    lang: String,
    author: Option<String>,
    /// What the site is, in the *default* language.
    description: Option<String>,
    /// Declared languages as `(code, display name)`, default first; empty on a
    /// single-language site.
    languages: Vec<(String, Option<String>)>,
    /// The syndication feeds this build writes, empty when it writes none.
    feeds: Vec<FeedInfo>,
}

/// One syndication feed the build publishes, so a template links what was
/// actually written rather than a conventional name it guessed at.
#[derive(Debug, Clone)]
struct FeedInfo {
    /// The format's config name: `rss`, `atom`, `json`.
    format: &'static str,
    /// The media type a `<link rel="alternate">` announces it under.
    mime: &'static str,
    /// Its root-relative URL per language code, since each language gets its
    /// own feed under its own scope.
    urls: Vec<(String, String)>,
}

impl BuildContext {
    /// The field naming the build date inside the metadata dictionary.
    pub(super) const DATE: &'static str = "date";

    /// The context a build of `config`'s project would see right now, for the
    /// generators that run outside a build and still have to serve what one
    /// would.
    pub fn of(config: &Config) -> Self {
        let root = crate::fs::canonical(&config.root);
        let repo = Repo::discover(&root);
        Self::detect(
            repo.as_ref(),
            OffsetDateTime::now_utc(),
            config,
            Mode::Build,
        )
    }

    /// Detect build metadata for a site in `repo`, or in no repository at all.
    pub(super) fn detect(
        repo: Option<&Repo>,
        now: OffsetDateTime,
        config: &Config,
        mode: Mode,
    ) -> Self {
        Self {
            version: crate::VERSION,
            date: now.date().to_string(),
            mode,
            profile: config.profile.clone(),
            git: repo.and_then(Repo::head),
            site: SiteInfo {
                title: config.site.clone(),
                url: config.url.clone(),
                lang: config.lang.clone(),
                author: config.author.clone(),
                description: config.description(&config.lang).map(str::to_owned),
                languages: if config.multilingual() {
                    config
                        .langs()
                        .iter()
                        .map(|code| ((*code).to_owned(), config.name(code).map(str::to_owned)))
                        .collect()
                } else {
                    Vec::new()
                },
                feeds: SiteInfo::feeds(config),
            },
            client: codegen::Value::dict(config.client.iter().cloned()),
        }
    }

    /// The fields the `site` module exposes, in emission order: `version`
    /// first, then the `site` sub-tree's own keys.
    ///
    /// Read back off the injected tree, so a module cannot serve a second
    /// derivation from config, and in an order the generated JavaScript's
    /// named exports and object literal follow.
    pub(crate) fn site_fields(tree: &codegen::Value) -> Vec<(String, codegen::Value)> {
        let mut fields = Vec::new();
        if let Some(version) = tree.get("version") {
            fields.push(("version".to_owned(), version.clone()));
        }
        if let Some(codegen::Value::Dict(pairs)) = tree.get("site") {
            fields.extend(pairs.iter().cloned());
        }
        fields
    }
}

/// The dictionary placed at `sys.inputs.baudelaire`.
impl From<&BuildContext> for codegen::Value {
    fn from(cx: &BuildContext) -> Self {
        let mut fields = vec![
            ("version", Self::str(cx.version)),
            (BuildContext::DATE, Self::str(&cx.date)),
            ("mode", Self::str(cx.mode.as_str())),
        ];
        if let Some(profile) = &cx.profile {
            fields.push(("profile", Self::str(profile)));
        }
        if let Some(git) = &cx.git {
            fields.push(("git", git.into()));
        }
        fields.push(("site", (&cx.site).into()));
        fields.push(("client", cx.client.clone()));
        Self::dict(fields)
    }
}

/// Every key is present, `none` when unset, so `#import ..: author` reads
/// `none` on an authorless site rather than failing.
impl SiteInfo {
    /// The feeds `generate { feed { formats } }` asks for, each with the URL it
    /// lands at in every built language. Read off the same `file` and `scope`
    /// the emitter writes by, so a link here cannot name a file no pass wrote.
    fn feeds(config: &Config) -> Vec<FeedInfo> {
        let feed = &config.generate.feed;
        feed.formats
            .iter()
            .map(|kind| FeedInfo {
                format: crate::config::Named::name(*kind),
                mime: kind.mime(),
                urls: config
                    .langs()
                    .iter()
                    .map(|lang| {
                        let scope = config.scope(lang, "");
                        let dir = crate::config::Permalink::join(&[&scope]);
                        ((*lang).to_owned(), format!("{dir}{}", feed.file(*kind)))
                    })
                    .collect(),
            })
            .collect()
    }
}

impl From<&FeedInfo> for codegen::Value {
    fn from(feed: &FeedInfo) -> Self {
        Self::dict([
            ("format", Self::str(feed.format)),
            ("mime", Self::str(feed.mime)),
            (
                "urls",
                Self::dict(feed.urls.iter().map(|(lang, url)| (lang, Self::str(url)))),
            ),
        ])
    }
}

impl From<&SiteInfo> for codegen::Value {
    fn from(site: &SiteInfo) -> Self {
        let langs = site.languages.iter().map(|(code, name)| {
            Self::dict([
                ("code", Self::str(code)),
                ("name", Self::str(name.as_deref().unwrap_or(code))),
            ])
        });
        Self::dict([
            ("title", Self::opt(site.title.clone())),
            ("url", Self::opt(site.url.clone())),
            ("lang", Self::str(&site.lang)),
            ("author", Self::opt(site.author.clone())),
            ("description", Self::opt(site.description.clone())),
            ("languages", Self::array(langs)),
            ("feeds", Self::array(site.feeds.iter().map(Self::from))),
        ])
    }
}
