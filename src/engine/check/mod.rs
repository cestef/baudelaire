//! Post-render validation of the compiled pages.
//!
//! [`Links`] resolves internal `.typ` references and either fails the build or
//! downgrades to a [`Ui`] warning per its own config gate: the strict-vs-lenient
//! policy lives with the check rather than the caller.
//! [`external::External`] verifies outbound links over the network and
//! runs from `check` alone, so a build never depends on someone else's host.
//! Both are plain calls: there was a `Check` trait and a registry around a
//! single impl, and a trait with one implementation is not an abstraction (see
//! [`super::emit::Processors`] for the shape to restore if a third arrives).

mod external;

pub(in crate::engine) use external::External;

use std::path::Path;

use crate::config::Config;
use crate::error::{Broken, BrokenLinks, Result};
use crate::ui::Ui;

/// Read-only view of the freshly compiled pages handed to every check. Cached
/// pages are excluded by the caller: they kept their links from the build that
/// produced them, so there is nothing new to validate.
pub(super) struct Compiled<'a> {
    pub config: &'a Config,
    pub pages: &'a [CheckedPage<'a>],
}

/// One compiled page's validation-relevant facts. Extend this as new checks need
/// more of the compiled page (its HTML, its dependencies, ...).
pub(super) struct CheckedPage<'a> {
    /// The page's path relative to the content root, for diagnostics.
    pub label: String,
    /// The `.typ` source path, so a check can locate a span within it.
    pub source: &'a Path,
    /// Raw targets of the broken internal links this page produced.
    pub broken: &'a [String],
    /// Outbound `http(s)` link targets, empty unless external checking is on.
    pub external: &'a [String],
}

/// Broken internal `.typ` links: every reference must resolve to an existing
/// page. Fatal under `links.strict`, otherwise the identical diagnostic as a
/// warning.
pub(super) struct Links;

impl Links {
    pub(super) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        let broken: Vec<Broken> = site
            .pages
            .iter()
            .flat_map(|page| {
                page.broken
                    .iter()
                    .map(|target| Broken::new(page.label.clone(), target.clone(), page.source))
            })
            .collect();
        if broken.is_empty() {
            return Ok(());
        }
        if site.config.links.strict {
            return Err(BrokenLinks::new(broken).into());
        }
        ui.warn(BrokenLinks::warning(broken));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::error::BaudelaireErrorKind;
    use crate::ui::Level;

    /// One page carrying the given broken-link targets.
    fn page(broken: &[String]) -> CheckedPage<'_> {
        CheckedPage {
            label: "post.typ".into(),
            source: Path::new("post.typ"),
            broken,
            external: &[],
        }
    }

    #[test]
    fn links_pass_reports_nothing_when_all_resolve() {
        let config = Config::default();
        let ui = Ui::new(Level::Silent);
        let pages = [page(&[])];
        let site = Compiled {
            config: &config,
            pages: &pages,
        };

        Links::run(&site, &ui).unwrap();
        assert_eq!(ui.warnings(), 0);
    }

    #[test]
    fn strict_broken_links_fail_the_build() {
        let mut config = Config::default();
        config.links.strict = true;
        let ui = Ui::new(Level::Silent);
        let broken = ["/missing".to_owned()];
        let pages = [page(&broken)];
        let site = Compiled {
            config: &config,
            pages: &pages,
        };

        let err = Links::run(&site, &ui).unwrap_err();
        assert!(matches!(err, BaudelaireErrorKind::BrokenLinks(_)));
        assert_eq!(
            ui.warnings(),
            0,
            "a strict failure is an error, not a warning"
        );
    }

    #[test]
    fn lenient_broken_links_warn_without_failing() {
        let mut config = Config::default();
        config.links.strict = false;
        let ui = Ui::new(Level::Silent);
        let broken = ["/missing".to_owned(), "/gone".to_owned()];
        let pages = [page(&broken)];
        let site = Compiled {
            config: &config,
            pages: &pages,
        };

        Links::run(&site, &ui).unwrap();
        assert_eq!(ui.warnings(), 1, "all broken links fold into one warning");
    }
}
