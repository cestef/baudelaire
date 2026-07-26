//! Build warnings and advice: precise, typed diagnostics that never stop a run.
//!
//! Same discipline as the error types: every warning is its own struct with a
//! `baudelaire::..` code, typed fields, and a `help`, rendered by miette with
//! `Severity::Warning` (yellow) so it reads like an error report without being
//! one. Informational notes use `Severity::Advice`. Collected through
//! [`crate::ui::Ui::warn`] and rendered together at the end of the run.

use std::path::PathBuf;

use super::BaudelaireErrorKind;

/// `init`/`new` found the file already on disk and left it untouched.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("`{path}` already exists, left untouched")]
#[diagnostic(
    code(baudelaire::scaffold::exists),
    severity(warning),
    help("remove the file first if you want it re-scaffolded")
)]
pub struct ScaffoldExists {
    pub path: PathBuf,
}

/// `new` computed a permalink already produced by an existing page.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("`{url}` is already produced by {origin}")]
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
#[error("`{tool}` not found, repository setup skipped")]
#[diagnostic(
    code(baudelaire::vcs::missing),
    severity(warning),
    help("install it, or re-run `init --vcs` once it is on PATH")
)]
pub struct VcsMissing {
    pub tool: &'static str,
}

/// Two different externalized images resolved to the same served filename. Only
/// the first is kept, so one image would be wrong; the author must rename a
/// source or enable `output { assets { fingerprint } }` to disambiguate by
/// content hash.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("two images map to `{name}`: `{kept}` and `{dropped}`")]
#[diagnostic(
    code(baudelaire::images::collision),
    severity(warning),
    help(
        "rename one source, or turn on `output {{ assets {{ fingerprint }} }}` to name by content"
    )
)]
pub struct ImageCollision {
    pub name: String,
    pub kept: PathBuf,
    pub dropped: PathBuf,
}

/// Two pages claimed the same `redirect` old-path. Only the first stub is
/// written, so the second page's redirect silently does not exist. The common
/// cause is translating a page by copying its frontmatter, which copies the
/// `redirect` list along with it.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("two pages redirect `{old}`: `{kept}` and `{dropped}`")]
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
#[error("refusing to clean `{dir}`: it contains the project")]
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
#[error("host key for `{host}` has changed, and `strict #false` accepted it")]
#[diagnostic(
    code(baudelaire::deploy::ssh::host_key_accepted),
    severity(warning),
    help("confirm the new key out of band, run `ssh-keygen -R {host}`, and set `strict #true`")
)]
pub struct HostKeyAccepted {
    pub host: String,
}

/// The version-control tool ran but failed to initialize a repository.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("`{tool} init` failed, repository setup skipped{}", detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default())]
#[diagnostic(code(baudelaire::vcs::failed), severity(warning))]
pub struct VcsFailed {
    pub tool: &'static str,
    /// The tool's own stderr, when it printed one.
    pub detail: Option<String>,
}

/// `serve --open` could not launch a browser; the server is up regardless.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("could not open a browser at {url}")]
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
#[error("unreadable cache manifest at `{path}`, rebuilding from scratch")]
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
#[error("announce destination resolved to {did}")]
#[diagnostic(
    // Distinct from `announce::did`, the mismatch *error*.
    code(baudelaire::announce::did_unpinned),
    severity(advice),
    help(
        "pin it with `announce.standard.did \"{did}\"` in config.kdl to emit verification artifacts at build time"
    )
)]
pub struct DidUnpinned {
    pub did: String,
}
