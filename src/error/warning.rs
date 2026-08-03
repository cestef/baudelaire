//! Build warnings and advice: precise, typed diagnostics that never stop a run.
//!
//! Same discipline as the error types: every warning is its own struct with a
//! `baudelaire::..` code, typed fields, and a `help`, rendered by miette with
//! `Severity::Warning` (yellow) so it reads like an error report without being
//! one. Informational notes use `Severity::Advice`. Collected through
//! [`crate::ui::Ui::warn`] and rendered together at the end of the run.

use std::path::PathBuf;

use itertools::Itertools;

use super::BaudelaireErrorKind;
use crate::ui::{Code, Text};

/// `init`/`new` found the file already on disk and left it untouched.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} already exists, left untouched", Code(.path.display()))]
#[diagnostic(
    code(baudelaire::scaffold::exists),
    severity(warning),
    help("remove the file first if you want it re-scaffolded")
)]
pub struct ScaffoldExists {
    pub path: PathBuf,
}

/// `clean` could not load the config, and swept the built-in directories.
///
/// `clean` is the recovery command, so depending on the artifact most likely to
/// be broken is backwards: a config syntax error used to block the very wipe
/// that would let you start over. A config that is *missing* stays an error,
/// since sweeping `public` and `.baudelaire` out of whatever directory you
/// happened to be standing in is not a recovery.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not load the config; cleaning the default directories")]
#[diagnostic(
    code(baudelaire::clean::defaults),
    severity(warning),
    help("fix the config to sweep the directories it names, or pass explicit targets")
)]
pub struct CleanDefaults {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

/// `new` could not read the existing content, and scaffolded without it.
///
/// The inference is a convenience: the next `order` in an ordered collection
/// and the permalink-collision check. Neither is worth refusing to write a file
/// over, and one broken sibling page used to be enough to refuse.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not read the existing content; scaffolding without it")]
#[diagnostic(
    code(baudelaire::scaffold::uninferred),
    severity(warning),
    help(
        "the next `order` and the permalink-collision check are skipped; the page is written either way"
    )
)]
pub struct Uninferred {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

/// `new` computed a permalink already produced by an existing page.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} is already produced by {}", Code(.url), Text(.origin))]
#[diagnostic(
    code(baudelaire::scaffold::permalink_taken),
    severity(warning),
    help(
        "give the new page a distinct `slug` in its frontmatter, or place it under a different path"
    )
)]
pub struct PermalinkTaken {
    pub url: String,
    pub origin: String,
}

/// The requested version-control tool is not installed.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} not found, repository setup skipped", Code(.tool))]
#[diagnostic(
    code(baudelaire::vcs::missing),
    severity(warning),
    help("install it, or re-run `init --vcs` once it is on PATH")
)]
pub struct VcsMissing {
    pub tool: &'static str,
}

/// A table module was mirrored with no data in it, because the site has never
/// been built. Advice, not a warning: every symbol still resolves, so an editor
/// is served either way and only the values are missing.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{modules} have no data until the site has been built once")]
#[diagnostic(
    code(baudelaire::mirror::unbuilt),
    severity(advice),
    help("build the site, then mirror again to fill the tables in")
)]
pub struct MirrorUnbuilt {
    /// The modules, already marked up (see [`crate::ui::markup!`]): one code
    /// span each, so a list of them reads as a list rather than as one name
    /// with commas in it.
    pub modules: String,
}

/// The editor-tooling mirror could not be written during `init`. Nothing about
/// the site is affected: a build serves every generated module from memory, so
/// this only means an editor will mark the imports unresolved until
/// `baudelaire mirror` succeeds.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("modules not mirrored for editor tooling: {}", Text(.reason))]
#[diagnostic(
    code(baudelaire::mirror::skipped),
    severity(warning),
    help("the site still builds; run `baudelaire mirror --path <dir>` to choose a location")
)]
pub struct MirrorSkipped {
    pub reason: String,
}

/// Two different externalized images resolved to the same served filename. Only
/// the first is kept, so one image would be wrong; the author must rename a
/// source or enable `assets { fingerprint }` to disambiguate by
/// content hash.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error(
    "two images map to {}: {} and {}",
    Code(.name),
    Code(.kept.display()),
    Code(.dropped.display())
)]
#[diagnostic(
    code(baudelaire::images::collision),
    severity(warning),
    help("rename one source, or turn on `assets {{ fingerprint }}` to name by content")
)]
pub struct ImageCollision {
    pub name: String,
    pub kept: PathBuf,
    pub dropped: PathBuf,
}

/// An outbound link nothing answered for: DNS, TLS, a timeout, a refused
/// connection. A warning rather than an error because the likeliest cause is the
/// network in between, not the site.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{}: {}", Text(.url), Text(.why))]
#[diagnostic(code(baudelaire::links::unreachable), severity(warning))]
pub struct Unreachable {
    pub url: String,
    pub why: String,
}

/// The outbound links that could not be reached at all.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} outbound link{} could not be reached", links.len(), if links.len() == 1 { "" } else { "s" })]
#[diagnostic(
    code(baudelaire::links::unreachable),
    severity(warning),
    help("re-run to retry; a host that stays unreachable is worth checking by hand")
)]
pub struct UnreachableLinks {
    #[related]
    pub links: Vec<Unreachable>,
}

impl From<Vec<Unreachable>> for UnreachableLinks {
    fn from(links: Vec<Unreachable>) -> Self {
        Self { links }
    }
}

/// The single-file export's entry permalink matches no built page, so there is
/// no head or body to build the shell around and nothing is written.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("no page at {}, single-file export skipped", Code(.entry))]
#[diagnostic(
    code(baudelaire::output::standalone_entry),
    severity(warning),
    help("point `navigation {{ standalone {{ entry }} }}` at a permalink the site builds")
)]
pub struct StandaloneEntryMissing {
    pub entry: String,
}

/// The single-file export ran with `html { embed }` off, so its pages still
/// reference `/assets/..` by URL. The file is one document but not a
/// self-contained one: opened from disk, it loses its styles and images.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} links its assets instead of carrying them", Code(.file))]
#[diagnostic(
    code(baudelaire::output::standalone_linked),
    severity(warning),
    help("turn on `html {{ embed #true }}` to inline them into the file")
)]
pub struct StandaloneLinked {
    pub file: String,
}

/// Two pages claimed the same `redirect` old-path. Only the first stub is
/// written, so the second page's redirect silently does not exist. The common
/// cause is translating a page by copying its frontmatter, which copies the
/// `redirect` list along with it.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error(
    "two pages redirect {}: {} and {}",
    Code(.old),
    Code(.kept.display()),
    Code(.dropped.display())
)]
#[diagnostic(
    code(baudelaire::output::redirect_collision),
    severity(warning),
    help("keep the `redirect` entry on one page, or give each a distinct old path")
)]
pub struct RedirectCollision {
    pub old: String,
    pub kept: PathBuf,
    pub dropped: PathBuf,
}

/// `clean` was pointed at a directory that holds the project itself.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("refusing to clean {}: it contains the project", Code(.dir.display()))]
#[diagnostic(
    code(baudelaire::output::clean_refused),
    severity(warning),
    help("point `paths {{ dist }}` / `cache {{ dir }}` at a directory below the project root")
)]
pub struct CleanRefused {
    pub dir: PathBuf,
}

/// A host presented a key that does not match the one `known_hosts` records,
/// and `strict #false` accepted it. The connection went ahead; the point is
/// that it does not go ahead *in silence*, which turns a flag set once to
/// bootstrap into a permanently man-in-the-middle-accepting setting. OpenSSH
/// warns here too.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("host key for {} has changed, and `strict #false` accepted it", Code(.host))]
#[diagnostic(
    code(baudelaire::deploy::ssh::host_key_accepted),
    severity(warning),
    help(
        "confirm the new key out of band, run `ssh-keygen -R {}`, and set `strict #true`",
        Text(.host)
    )
)]
pub struct HostKeyAccepted {
    pub host: String,
}

/// The version-control tool ran but failed to initialize a repository.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error(
    "`{} init` failed, repository setup skipped{}",
    Text(.tool),
    detail.as_deref().map(|d| format!(": {}", Text(d))).unwrap_or_default()
)]
#[diagnostic(code(baudelaire::vcs::failed), severity(warning))]
pub struct VcsFailed {
    pub tool: &'static str,
    /// The tool's own stderr, when it printed one.
    pub detail: Option<String>,
}

/// `serve --open` could not launch a browser; the server is up regardless.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not open a browser at {}", Text(.url))]
#[diagnostic(code(baudelaire::serve::browser), severity(warning))]
pub struct BrowserOpen {
    pub url: String,
    #[source]
    pub source: std::io::Error,
}

/// The file watcher dropped events or a watch; the server keeps serving.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("file watcher error, some changes may not trigger a rebuild")]
#[diagnostic(
    // Distinct from `serve::watch`, which is the *failure to establish* a watch:
    // codes are what a user greps and suppresses by, so two conditions must not
    // share one.
    code(baudelaire::serve::watch_lost),
    severity(warning),
    help("restart `baudelaire serve` to re-establish the watches")
)]
pub struct WatchLost {
    #[source]
    pub source: notify::Error,
}

/// `config.kdl` changed but no longer parses; the dev server keeps the last
/// good config so it stays up.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("config reload failed, keeping the last good config")]
#[diagnostic(code(baudelaire::serve::reload), severity(warning))]
pub struct ConfigReload {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

/// A dev-server rebuild failed; the previous output stays served.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("rebuild failed, still serving the previous build")]
#[diagnostic(code(baudelaire::serve::rebuild), severity(warning))]
pub struct RebuildFailed {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

/// A cache manifest that exists but does not parse (torn write, corruption,
/// manual edit): ignored, forcing a full rebuild.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("unreadable cache manifest at {}, rebuilding from scratch", Code(.path.display()))]
#[diagnostic(
    code(baudelaire::cache::manifest),
    severity(warning),
    help("`baudelaire clean --cache` clears it for good")
)]
pub struct ManifestUnreadable {
    pub path: PathBuf,
    #[source]
    pub source: serde_json::Error,
}

/// A site whose backlinks never settled.
///
/// A page's backlinks are made true by compiling it again, which changes what
/// *it* links to only if its content branches on them. When it does, every
/// repair moves the graph and there is no answer to converge on. The build ships
/// the last one it computed rather than looping, and says which pages are still
/// disagreeing with it.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("backlinks did not settle: {} still disagree with the site's links", .pages.len())]
#[diagnostic(
    code(baudelaire::backlinks::unstable),
    severity(warning),
    help(
        "these pages link somewhere different depending on their own `page.backlinks`: {}",
        .pages.iter().map(Code).format(", ")
    )
)]
pub struct BacklinksUnstable {
    /// The pages still disagreeing, project-relative.
    pub pages: Vec<String>,
}

/// A `generate { manifest }` block that names no icon.
///
/// The manifest is still written, and it is still valid: the members it lacks
/// are what a browser needs to *install* the site, so without them the file is
/// emitted, linked from every page, and nothing ever offers to add the site to a
/// home screen. That silence is exactly what a green build should not leave
/// unexplained.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("the web app manifest names no icon, so the site cannot be installed")]
#[diagnostic(
    code(baudelaire::manifest::icons),
    severity(warning),
    help(
        "add `icons {{ \"/icon-512.png\" size=512 }}` to `generate {{ manifest }}`; \
         a launcher wants a 192 and a 512"
    )
)]
pub struct ManifestIcons;

/// A feature that needs the site's public address found no `url` in config.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("no `url` configured: {feature} {effect}")]
#[diagnostic(
    code(baudelaire::config::url),
    severity(warning),
    help("set `url \"https://example.com\"` in config.kdl")
)]
pub struct BaseUrlMissing {
    pub feature: &'static str,
    /// What happened instead: `skipped`, `emitted with relative links`, ..
    pub effect: &'static str,
}

/// The site's config asked for a capability this binary was not compiled with.
///
/// Every optional feature removes capability silently: a `.css` file is copied
/// verbatim, a card is never drawn, an image is never re-encoded. The build is
/// green either way, so without this the author only learns from the output.
/// One struct rather than one per feature, like [`BaseUrlMissing`]: the
/// condition is the same in every case and only the nouns change.
#[derive(thiserror::Error, miette::Diagnostic, Debug, Clone, Copy)]
#[error(
    "{} needs the {} feature, which this build lacks: {effect}",
    Code(.setting),
    Code(.cargo)
)]
#[diagnostic(
    code(baudelaire::feature::missing),
    severity(warning),
    help(
        "rebuild with `--features {}`, or drop {} from config.kdl",
        Text(.cargo),
        Code(.setting)
    )
)]
pub struct FeatureMissing {
    /// The config that asked, spelled as the author writes it in `config.kdl`.
    pub setting: &'static str,
    /// The cargo feature that compiles the capability in.
    pub cargo: &'static str,
    /// What the build does instead: `stylesheets are copied unminified`, ..
    pub effect: &'static str,
}

/// A setting that does nothing, because the setting it depends on is off.
///
/// The counterpart of [`FeatureMissing`] for settings gated by *each other*
/// rather than by a cargo feature. Same shape for the same reason: the config
/// was accepted, the build was green, and the only evidence that a key had no
/// effect was its absence from the output.
#[derive(thiserror::Error, miette::Diagnostic, Debug, Clone, Copy)]
#[error("{} does nothing without {}: {effect}", Code(.setting), Code(.needs))]
#[diagnostic(code(baudelaire::config::inert), severity(warning), help("{help}"))]
pub struct SettingInert {
    /// The setting that was asked for, spelled as it is written in `config.kdl`.
    pub setting: &'static str,
    /// What it needs and does not have.
    pub needs: &'static str,
    /// What the build produces instead.
    pub effect: &'static str,
    /// How to make it take effect, or how to stop asking.
    pub help: &'static str,
}

/// Pages a announce run skipped because they carry no publication date.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} skipped: no publication date", crate::ui::Count::pages(*count))]
#[diagnostic(
    code(baudelaire::announce::undated),
    severity(warning),
    help("add `date` to their frontmatter (run with -v to see which pages)")
)]
pub struct Undated {
    pub count: usize,
}

/// The account's DID is not pinned in config; worth doing, not wrong.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("announce destination resolved to {}", Text(.did))]
#[diagnostic(
    // Distinct from `announce::did`, the mismatch *error*.
    code(baudelaire::announce::did_unpinned),
    severity(advice),
    help(
        "pin it with `announce.standard.did \"{}\"` in config.kdl to emit verification artifacts at build time",
        Text(.did)
    )
)]
pub struct DidUnpinned {
    pub did: String,
}
