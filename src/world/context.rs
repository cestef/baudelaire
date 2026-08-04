//! Build metadata: what a page can learn about the build that produced it.
//!
//! [`BuildContext`] is detected once per build and injected at
//! `sys.inputs.baudelaire`, and mirrored into the `site` modules on both sides
//! (`@baudelaire/site` and `baudelaire:site`) by [`BuildContext::site_fields`].
//! Everything here is a plain data tree lowered through [`codegen::Value`], so
//! one description drives the injected value, the generated module, and the
//! per-page read tracking in [`crate::graph::access`].

use std::path::Path;
use std::process::Command;

use time::OffsetDateTime;

use crate::codegen;
use crate::config::Config;

/// How the site is being produced, exposed to pages as
/// `sys.inputs.baudelaire.mode` so they can branch (e.g. a dev banner in serve).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Build,
    Serve,
    Check,
}

impl Mode {
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
/// It no longer keys the cache directly: pages read individual values from it,
/// and [`crate::graph::Analyzer`] tracks those reads per page, so a commit or a
/// new day invalidates only the pages that display the value that moved. The
/// tree is exposed for that tracking via [`crate::world::Project::tracked`].
#[derive(Debug, Clone)]
pub struct BuildContext {
    version: &'static str,
    date: String,
    mode: Mode,
    profile: Option<String>,
    git: Option<GitInfo>,
    site: SiteInfo,
    /// The `client { }` constants, also exposed to templates at
    /// `sys.inputs.baudelaire.client` (mirroring the `baudelaire:config` module).
    client: codegen::Value,
}

/// Git state of the site repository at build time.
#[derive(Debug, Clone)]
struct GitInfo {
    /// The full commit SHA: pages slice it themselves for a short form.
    hash: String,
    /// The revision number: how many commits are reachable from HEAD.
    rev: Option<String>,
    branch: Option<String>,
    tag: Option<String>,
    committed: Option<String>,
    dirty: bool,
}

/// A mirror of site identity, so layouts can read it via `sys.inputs` without
/// per-page frontmatter plumbing.
#[derive(Debug, Clone)]
struct SiteInfo {
    title: Option<String>,
    url: Option<String>,
    lang: String,
    author: Option<String>,
    /// What the site is, in the *default* language. A page's own language
    /// override reaches a feed through `Config::description`; a template that
    /// needs the localized one reads it from its own frontmatter or strings.
    description: Option<String>,
    /// Declared languages as `(code, display name)`, default first. Empty on a
    /// single-language site, so `site.languages` only appears when i18n is on.
    languages: Vec<(String, Option<String>)>,
}

impl BuildContext {
    /// The field naming the build date inside the metadata dictionary. Named
    /// once: the dictionary is built from it and [`crate::world::Project::clock`]
    /// composes the tracked key from it.
    pub(super) const DATE: &'static str = "date";

    /// The context a build of `config`'s project would see right now.
    ///
    /// For the generators that run *outside* a build and still have to serve
    /// what one would: the `@baudelaire/*` typst packages and the `baudelaire:*`
    /// TypeScript declarations, both written by `baudelaire packages`.
    pub fn of(config: &Config) -> Self {
        Self::detect(
            &crate::fs::canonical(&config.root),
            OffsetDateTime::now_utc(),
            config,
            Mode::Build,
        )
    }

    /// Detect build metadata for the site rooted at `root`.
    pub(super) fn detect(root: &Path, now: OffsetDateTime, config: &Config, mode: Mode) -> Self {
        Self {
            version: crate::VERSION,
            date: now.date().to_string(),
            mode,
            profile: config.profile.clone(),
            git: GitInfo::detect(root),
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
            },
            client: codegen::Value::dict(config.client.iter().cloned()),
        }
    }

    /// The fields the `site` module exposes, in emission order: `version`
    /// first, then the `site` sub-tree's own keys.
    ///
    /// The single owner of that projection. Both registries serve it, the Typst
    /// one as `@baudelaire/site` bindings ([`super::module`]) and the JavaScript
    /// one as `baudelaire:site` exports ([`crate::engine::asset::module`]), and
    /// spelling it twice had already let their key order drift apart.
    ///
    /// It reads the lowered tree rather than `&self` because that is all either
    /// registry holds, and because reading it back is the point: a module must
    /// serve the value injected at `sys.inputs.baudelaire`, never a second
    /// derivation from config that could disagree with it.
    ///
    /// Key order is cosmetic in a Typst dict, but the generated JavaScript is
    /// where it shows: it fixes the order of the named exports, of the object
    /// literal, and hence of `JSON.stringify`. `version` leads because that is
    /// its position in the injected tree.
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

/// The dictionary placed at `sys.inputs.baudelaire`, built once as a
/// [`codegen::Value`] and converted to a Typst runtime value at injection.
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

impl GitInfo {
    /// Read git state via the `git` CLI, or `None` outside a repository.
    fn detect(root: &Path) -> Option<Self> {
        // One call for the commit's own fields: `git` startup dominates each of
        // these, so asking for two lines beats two processes.
        let head = Self::run(root, &["log", "-1", "--format=%H%n%cI"])?;
        let (hash, committed) = head.split_once('\n')?;
        Some(Self {
            hash: hash.to_owned(),
            committed: Some(committed.to_owned()),
            rev: Self::run(root, &["rev-list", "--count", "HEAD"]),
            branch: Self::run(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
            // No `--always`: it falls back to a bare commit hash in a tagless
            // repo, which would populate `git.tag` with a non-tag. An empty
            // output (no tag reachable) becomes `None` instead.
            tag: Self::run(root, &["describe", "--tags"]),
            // A non-empty `status --porcelain` means uncommitted changes.
            // `--no-renames` skips rename detection, which is pure overhead
            // when the answer is only "is anything different at all".
            dirty: Self::run(root, &["status", "--porcelain", "--no-renames"]).is_some(),
        })
    }

    /// Run a `git` command in `root`, returning its trimmed stdout, or `None`
    /// if git is absent, the command fails, or the output is empty. The single
    /// place this crate shells out to git.
    fn run(root: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }
}

impl From<&GitInfo> for codegen::Value {
    fn from(git: &GitInfo) -> Self {
        let mut fields = vec![("hash", Self::str(&git.hash))];
        if let Some(rev) = &git.rev {
            fields.push(("rev", Self::str(rev)));
        }
        if let Some(branch) = &git.branch {
            fields.push(("branch", Self::str(branch)));
        }
        if let Some(tag) = &git.tag {
            fields.push(("tag", Self::str(tag)));
        }
        if let Some(committed) = &git.committed {
            fields.push(("committed", Self::str(committed)));
        }
        fields.push(("dirty", Self::Bool(git.dirty)));
        Self::dict(fields)
    }
}

/// Every key is present, `none` when unset, so this is also the key list
/// `@baudelaire/site` exports (see [`super::module`]): a module binding must
/// exist whether or not the site configures it, or `#import ..: author` would
/// fail on an authorless site instead of reading `none`.
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
        ])
    }
}
