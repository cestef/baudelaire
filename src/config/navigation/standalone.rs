//! `navigation { standalone { } }`: the whole site as one file.

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Single-file export: the whole site inlined into one HTML document, each
/// page a route the bundled router swaps in.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct StandaloneConfig {
    /// Whether to emit the single-file export.
    pub enabled: bool,

    /// The single file the site is written to.
    ///
    /// Relative to `dist`.
    #[key(contained)]
    pub file: String,

    /// The page that file opens on.
    ///
    /// The permalink of the page whose `<head>` and body seed the shell, the
    /// only route that renders without JavaScript. `None` means the site home
    /// (`/`, localized to `lang`).
    #[key(opt text)]
    pub entry: Option<String>,

    /// How it addresses pages once opened.
    #[key(choice(Router))]
    pub router: Router,
}

/// How a router represents the active route in the URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Router {
    /// `#/blog/post/`: the only mode that survives `file://`.
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
        Self {
            enabled: false,
            file: "site.html".into(),
            entry: None,
            router: Router::default(),
        }
    }
}
