//! `navigation { standalone { } }`: the whole site as one file.

use crate::config::Named;
use crate::config::dispatch::Kind::{Choice, Path, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Single-file export: the whole site inlined into one HTML document, each
/// page a route the bundled router swaps in. Enabled by the presence of a
/// `navigation { standalone { .. } }` block.
#[derive(Debug, Clone, Hash)]
pub struct StandaloneConfig {
    /// Whether to emit the single-file export.
    pub enabled: bool,
    /// Output file name, relative to `dist`.
    pub file: String,
    /// Permalink of the page whose `<head>` and body seed the shell: the route
    /// shown before any navigation, and the only one that renders without
    /// JavaScript. `None` means the site home (`/`, localized to `lang`).
    pub entry: Option<String>,
    /// How the router encodes the current route in the address bar.
    pub router: Router,
}

/// How a router represents the active route in the URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Router {
    /// `#/blog/post/`: the only mode that survives `file://`, where a single
    /// file is normally opened, since it never asks the browser for a path the
    /// filesystem has to have.
    #[default]
    Hash,
    /// `/blog/post/`, through the History API. Needs the file served by a host
    /// that answers every route with it.
    History,
}

impl Named for Router {
    const NAMES: &'static [(&'static str, Self)] =
        &[("hash", Self::Hash), ("history", Self::History)];
}

impl Default for StandaloneConfig {
    fn default() -> Self {
        // opt-in: the presence of a `standalone { .. }` block flips `enabled`.
        // The rest are the values that block inherits when it stays silent.
        Self {
            enabled: false,
            file: "site.html".into(),
            // no default entry: the site home, resolved against the pages that
            // actually built, so a site with no `/` still gets a shell.
            entry: None,
            router: Router::default(),
        }
    }
}

/// The `standalone { file ..; entry ..; router .. }` block. Its presence
/// enables the single-file export; the rest keep their defaults unless named.
impl Section for StandaloneConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "file",
            Path,
            "The single file the site is written to.",
            |c, n, t| {
                c.file = n.contained(t)?;
                Ok(())
            },
        ),
        ("entry", Text, "The page that file opens on.", |c, n, t| {
            c.entry = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "router",
            Choice(Router::names),
            "How it addresses pages once opened.",
            |c, n, t| {
                c.router = n.arg(t, 0)?.one::<Router>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
    ]);
}
