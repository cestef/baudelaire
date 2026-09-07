//! Build warnings and advice: typed diagnostics that never stop a run,
//! collected through [`crate::ui::Ui::warn`] and rendered at the end of it.

use std::path::PathBuf;

use itertools::Itertools;

use super::BaudelaireErrorKind;
use crate::ui::{Code, Text};

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

/// A config that is *missing* stays an error: sweeping the built-in
/// directories out of wherever you were standing is not a recovery.
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

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("the build produced no pages; nothing was pruned")]
#[diagnostic(
    code(baudelaire::prune::empty),
    severity(warning),
    help("`dist` is left as it was: a build with nothing in it cannot say what is orphaned")
)]
pub struct PruneEmpty;

/// Advice, not a warning: leaving a draft out is what a draft is for, and must
/// not count against `--strict`.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} not published", .0)]
#[diagnostic(
    code(baudelaire::content::held),
    severity(advice),
    help(
        "drafts build with `content {{ drafts {{ build #true }} }}` and future-dated pages with `content {{ future #true }}`; an expired one is out for good"
    )
)]
pub struct PagesHeld(pub crate::content::Held);

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

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} not found, repository setup skipped", Code(.tool))]
#[diagnostic(
    code(baudelaire::vcs::missing),
    severity(warning),
    // `--vcs` takes a value, so the tool's own name is what re-runs what was
    // asked for; the bare flag would be a usage error.
    help("install it, or re-run `init --vcs {}` once it is on PATH", Text(.tool))
)]
pub struct VcsMissing {
    pub tool: &'static str,
}

/// The tool is on `PATH` but could not be started, unlike [`VcsMissing`]
/// (absent) and [`VcsFailed`] (running, and refusing the repository).
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not run {}, repository setup skipped", Code(.tool))]
#[diagnostic(
    code(baudelaire::vcs::unrunnable),
    severity(warning),
    help("initialize the repository by hand; the project is scaffolded either way")
)]
pub struct VcsUnrunnable {
    pub tool: &'static str,
    #[source]
    pub source: std::io::Error,
}

/// Advice, not a warning: every symbol still resolves, and only the values are
/// missing.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{modules} have no data until the site has been built once")]
#[diagnostic(
    code(baudelaire::mirror::unbuilt),
    severity(advice),
    help("build the site, then mirror again to fill the tables in")
)]
pub struct MirrorUnbuilt {
    /// Already marked up (see [`crate::ui::markup!`]), not text to be escaped.
    pub modules: String,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not load the config; mirroring the built-in defaults")]
#[diagnostic(
    code(baudelaire::mirror::defaults),
    severity(warning),
    help("fix the config and mirror again; `site` carries the defaults until you do")
)]
pub struct MirrorDefaults {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

/// A build serves every generated module from memory, so only an editor is
/// affected.
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

/// A warning rather than an error, because the likeliest cause is the network
/// in between and not the site.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{}: {}", Text(.url), Text(.why))]
#[diagnostic(code(baudelaire::links::unreachable), severity(warning))]
pub struct Unreachable {
    pub url: String,
    pub why: String,
}

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

/// The connection went ahead; the point is that it does not go ahead in
/// silence, which would make a bootstrap flag a permanent hole.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("host key for {} has changed, and `strict #false` accepted it", Code(.host))]
#[diagnostic(
    code(baudelaire::deploy::ssh::host_key_accepted),
    severity(warning),
    help(
        "confirm the new key out of band, run `ssh-keygen -R {}`, and set `strict #true`",
        Text(.entry)
    )
)]
pub struct HostKeyAccepted {
    pub host: String,
    /// Carried already built, because a miette `help` substitutes field names
    /// and cannot call [`crate::error::DeployError::entry`] itself.
    pub entry: String,
}

/// Trust on first use is still trust granted without evidence, so the key it
/// was granted to is named where an operator can compare it out of band.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("trusting {} on first use, with key {}", Code(.host), Code(.fingerprint))]
#[diagnostic(
    code(baudelaire::deploy::ssh::host_key_learned),
    severity(warning),
    help("compare that fingerprint against the host's own before the next deploy")
)]
pub struct HostKeyLearned {
    pub host: String,
    pub fingerprint: String,
}

/// `strict #false` accepting a key nothing could be compared against, which
/// otherwise reads like an ordinary connection.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not check the host key for {}, and `strict #false` accepted {}", Code(.host), Code(.fingerprint))]
#[diagnostic(
    code(baudelaire::deploy::ssh::host_key_unverified),
    severity(warning),
    help("make `~/.ssh/known_hosts` readable so the key can be compared")
)]
pub struct HostKeyUnverified {
    pub host: String,
    pub fingerprint: String,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error(
    "`{} init` failed, repository setup skipped{}",
    Text(.tool),
    detail.as_deref().map(|d| format!(": {}", Text(d))).unwrap_or_default()
)]
#[diagnostic(code(baudelaire::vcs::failed), severity(warning))]
pub struct VcsFailed {
    pub tool: &'static str,
    pub detail: Option<String>,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not open a browser at {}", Text(.url))]
#[diagnostic(code(baudelaire::serve::browser), severity(warning))]
pub struct BrowserOpen {
    pub url: String,
    #[source]
    pub source: std::io::Error,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("file watcher error, some changes may not trigger a rebuild")]
#[diagnostic(
    // Distinct from `serve::watch`, the failure to *establish* a watch: a code
    // is what a user greps by, so two conditions cannot share one.
    code(baudelaire::serve::watch_lost),
    severity(warning),
    help("restart `baudelaire serve` to re-establish the watches")
)]
pub struct WatchLost {
    #[source]
    pub source: notify::Error,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("config reload failed, keeping the last good config")]
#[diagnostic(code(baudelaire::serve::reload), severity(warning))]
pub struct ConfigReload {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("rebuild failed, still serving the previous build")]
#[diagnostic(code(baudelaire::serve::rebuild), severity(warning))]
pub struct RebuildFailed {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

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

/// A page whose content branches on its own backlinks moves the graph every
/// time it is repaired, so the build ships the last set rather than looping.
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
    pub pages: Vec<String>,
}

/// The manifest is still written and still valid; icons are only what a
/// browser needs to *install* the site.
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

/// An optional feature removes capability silently and the build is green
/// either way, so without this the author only learns from the output.
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
    /// Spelled as the author writes it in `config.kdl`.
    pub setting: &'static str,
    pub cargo: &'static str,
    /// What the build does instead: `stylesheets are copied unminified`, ..
    pub effect: &'static str,
}

/// The counterpart of [`FeatureMissing`] for settings gated by *each other*
/// rather than by a cargo feature.
#[derive(thiserror::Error, miette::Diagnostic, Debug, Clone, Copy)]
#[error("{} does nothing without {}: {effect}", Code(.setting), Code(.needs))]
#[diagnostic(code(baudelaire::config::inert), severity(warning), help("{help}"))]
pub struct SettingInert {
    /// Spelled as it is written in `config.kdl`.
    pub setting: &'static str,
    pub needs: &'static str,
    pub effect: &'static str,
    pub help: &'static str,
}

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

/// The static tree wins the rule file, so stubs are written instead and the old
/// URLs keep working under the other mechanism.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} is your own file, so redirects were written as stubs", Code(.path.display()))]
#[diagnostic(
    code(baudelaire::output::redirects_shadowed),
    severity(warning),
    help(
        "merge the generated rules into it by hand, or drop `generate {{ redirects }}` and keep the stubs"
    )
)]
pub struct RedirectsShadowed {
    pub path: PathBuf,
}

/// One file cannot be two feeds, and the site feed is the more inclusive, so it
/// keeps the path.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error(
    "{} is mounted where a site feed is, so it has no feed of its own",
    Code(.collection)
)]
#[diagnostic(
    code(baudelaire::feed::mounted),
    severity(warning),
    help(
        "the site feed already carries these pages: drop `feed` from the collection, or mount its index somewhere of its own"
    )
)]
pub struct FeedMounted {
    pub collection: String,
}

/// Advice rather than a warning: a host can supply one, and a site that means
/// to let it is not doing anything wrong.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("no not-found page: an unmatched URL gets the host's, not yours")]
#[diagnostic(
    code(baudelaire::content::not_found),
    severity(advice),
    help(
        "write {}, which publishes as {}",
        Code("content/404.typ"),
        Code(crate::config::Config::NOT_FOUND)
    )
)]
pub struct NotFoundMissing;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("announce destination resolved to {}", Text(.did))]
#[diagnostic(
    // Distinct from `announce::did`, the mismatch *error*.
    code(baudelaire::announce::did_unpinned),
    severity(advice),
    // The nested block, not `announce.standard.did`: the parser refuses a
    // dotted key, so that spelling is one config.kdl never accepted.
    help(
        "pin it with `announce {{ standard {{ did \"{}\" }} }}` in config.kdl to emit verification artifacts at build time",
        Text(.did)
    )
)]
pub struct DidUnpinned {
    pub did: String,
}

/// About the scheme in `config.kdl`, not the transport: `http://` is what opts
/// out of TLS, and a warning rather than a refusal because `localhost` exists.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} is plain HTTP, so {} is sent in clear", Code(.url), Text(.secret))]
#[diagnostic(
    code(baudelaire::remote::plaintext),
    severity(warning),
    help(
        "write {} as `https://`, unless it is a local server you control end to end",
        Code(.setting)
    )
)]
pub struct PlaintextEndpoint {
    /// Spelled as the author writes it in `config.kdl`.
    pub setting: &'static str,
    pub url: String,
    /// What travels over it: `the app password`, ..
    pub secret: &'static str,
}

/// Reported rather than dropped: a key the reconcile can neither overwrite nor
/// delete would otherwise sit on the remote with nothing ever naming it.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error(
    "{} on {} cannot be reached from here",
    crate::ui::Count::files(*.count),
    Code(.target)
)]
#[diagnostic(
    code(baudelaire::deploy::refused),
    severity(warning),
    help(
        "they are neither uploaded over nor deleted: their keys are not paths this can join \
         safely (a `//`, a trailing `/`, a `..`). Run with `-v` to see them"
    )
)]
pub struct RemotePathsRefused {
    pub count: usize,
    pub target: String,
}

/// Reported rather than passed off as a complete inventory: what the host did
/// send stands for its whole tree, and the reconcile acts on that.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} could not be listed in full", Code(.target))]
#[diagnostic(
    code(baudelaire::deploy::listing),
    severity(warning),
    help(
        "the host answered with a failure, so what it did send stands for the whole tree: files \
         it did not mention are uploaded again and never deleted. Run with `-v` for the command"
    )
)]
pub struct RemoteListingPartial {
    pub target: String,
}
