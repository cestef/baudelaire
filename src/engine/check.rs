//! Post-render validation passes.
//!
//! Each [`Check`] inspects the freshly compiled pages and reports problems it
//! finds. A check either fails the build (returns `Err`) or downgrades to a
//! [`Ui`] warning, per its own config gate — the strict-vs-lenient policy lives
//! with the check, not the runner. [`Checks::builtin`] is the single source of
//! what runs, in order: a new validation is one `impl Check` plus one line
//! there, exactly like [`super::process::Processors`] for emitted output.

use std::path::Path;

use crate::config::Config;
use crate::error::{Broken, BrokenLinks, Result};
use crate::ui::Ui;

/// Read-only view of the freshly compiled pages handed to every check. Cached
/// pages are excluded by the caller — they kept their links from the build that
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
}

/// One post-render validation pass.
pub(super) trait Check {
    /// Whether to run, from config alone — keeps the gate declarative and out of
    /// [`Check::run`]. Default: always.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    /// Validate the compiled site. `Err` fails the build; softer problems are
    /// surfaced on `ui` as warnings. Called only when [`Check::enabled`].
    fn run(&self, site: &Compiled, ui: &Ui) -> Result<()>;
}

/// Broken internal `.typ` links: every reference must resolve to an existing
/// page. Fatal under `links.strict`, otherwise the identical diagnostic as a
/// warning.
struct Links;

impl Check for Links {
    fn run(&self, site: &Compiled, ui: &Ui) -> Result<()> {
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

/// The built-in checks, in run order. THE single source of what validates the
/// compiled site: add a pass by adding one line here.
pub(super) struct Checks(Vec<Box<dyn Check>>);

impl Checks {
    pub(super) fn builtin() -> Self {
        Self(vec![Box::new(Links)])
    }

    /// Run each enabled check in order; the first failing check stops the build.
    pub(super) fn run(&self, site: &Compiled, ui: &Ui) -> Result<()> {
        for check in &self.0 {
            if check.enabled(site.config) {
                check.run(site, ui)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;

    use super::*;
    use crate::error::BaudelaireErrorKind;
    use crate::ui::Level;

    /// A check that logs its label when it runs, gated by a fixed flag.
    struct Marker(&'static str, bool, Rc<RefCell<Vec<&'static str>>>);

    impl Check for Marker {
        fn enabled(&self, _config: &Config) -> bool {
            self.1
        }

        fn run(&self, _site: &Compiled, _ui: &Ui) -> Result<()> {
            self.2.borrow_mut().push(self.0);
            Ok(())
        }
    }

    /// One page carrying the given broken-link targets.
    fn page(broken: &[String]) -> CheckedPage<'_> {
        CheckedPage {
            label: "post.typ".into(),
            source: Path::new("post.typ"),
            broken,
        }
    }

    #[test]
    fn registry_runs_only_enabled_checks_in_order() {
        let config = Config::default();
        let ui = Ui::new(Level::Silent);
        let log = Rc::new(RefCell::new(Vec::new()));
        let registry = Checks(vec![
            Box::new(Marker("first", true, log.clone())),
            Box::new(Marker("skipped", false, log.clone())),
            Box::new(Marker("last", true, log.clone())),
        ]);

        let site = Compiled {
            config: &config,
            pages: &[],
        };
        registry.run(&site, &ui).unwrap();

        assert_eq!(*log.borrow(), ["first", "last"]);
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

        Links.run(&site, &ui).unwrap();
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

        let err = Links.run(&site, &ui).unwrap_err();
        assert!(matches!(err, BaudelaireErrorKind::BrokenLinks(_)));
        assert_eq!(ui.warnings(), 0, "a strict failure is an error, not a warning");
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

        Links.run(&site, &ui).unwrap();
        assert_eq!(ui.warnings(), 1, "all broken links fold into one warning");
    }
}
