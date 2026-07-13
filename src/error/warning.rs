//! Build warnings and advice: precise, typed diagnostics that never stop a run.
//!
//! Same discipline as the error types — every warning is its own struct with a
//! `baudelaire::..` code, typed fields, and a `help`, rendered by miette with
//! `Severity::Warning` (yellow) so it reads like an error report without being
//! one. Informational notes use `Severity::Advice`. Collected through
//! [`crate::ui::Ui::warn`] and rendered together at the end of the run.

use std::path::PathBuf;

use super::BaudelaireErrorKind;

/// `init`/`new` found the file already on disk and left it untouched.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("`{path}` already exists — left untouched")]
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
#[error("`{tool}` not found — repository setup skipped")]
#[diagnostic(
    code(baudelaire::vcs::missing),
    severity(warning),
    help("install it, or re-run `init --vcs` once it is on PATH")
)]
pub struct VcsMissing {
    pub tool: &'static str,
}

/// The version-control tool ran but failed to initialize a repository.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("`{tool} init` failed — repository setup skipped{}", detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default())]
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
#[error("file watcher error — some changes may not trigger a rebuild")]
#[diagnostic(
    code(baudelaire::serve::watch),
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
#[error("config reload failed — keeping the last good config")]
#[diagnostic(code(baudelaire::serve::reload), severity(warning))]
pub struct ConfigReload {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

/// A dev-server rebuild failed; the previous output stays served.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("rebuild failed — still serving the previous build")]
#[diagnostic(code(baudelaire::serve::rebuild), severity(warning))]
pub struct RebuildFailed {
    #[related]
    pub errors: Vec<BaudelaireErrorKind>,
}

/// A cache manifest that exists but does not parse (torn write, corruption,
/// manual edit) — ignored, forcing a full rebuild.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("unreadable cache manifest at `{path}` — rebuilding from scratch")]
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
#[error("no `url` configured — {feature} {effect}")]
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

/// Pages a publish run skipped because they carry no publication date.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} skipped — no publication date", crate::ui::Count::pages(*count))]
#[diagnostic(
    code(baudelaire::publish::undated),
    severity(warning),
    help("add `date` to their frontmatter (run with -v to see which pages)")
)]
pub struct Undated {
    pub count: usize,
}

/// The account's DID is not pinned in config — worth doing, not wrong.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("publish destination resolved to {did}")]
#[diagnostic(
    code(baudelaire::publish::did),
    severity(advice),
    help(
        "pin it with `publish.standard.did \"{did}\"` in config.kdl to emit verification artifacts at build time"
    )
)]
pub struct DidUnpinned {
    pub did: String,
}
