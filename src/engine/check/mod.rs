//! Post-render validation of the compiled pages.
//!
//! [`Links`] resolves internal `.typ` references and either fails the build or
//! downgrades to a [`Ui`] warning per its own config gate: the strict-vs-lenient
//! policy lives with the check rather than the caller.
//! [`external::External`] verifies outbound links over the network and
//! runs from `check` alone, so a build never depends on someone else's host.
//! [`lint::Lints`] reports what the per-page DOM pass found, and
//! [`lint::Budgets`] weighs each page against `lint { budget { } }`.
//! All are plain calls: there was a `Check` trait and a registry around a
//! single impl, and a trait with one implementation is not an abstraction (see
//! [`super::emit::Processors`] for the shape to restore if the calls ever need
//! to be selected rather than listed).

mod external;
mod lint;

pub(in crate::engine) use external::External;
pub(in crate::engine) use lint::{Budgets, Lints};

use std::path::Path;

use crate::config::Config;
use crate::error::{Broken, BrokenLinks, Result};
use crate::render::{Emitted, Finding, Weight};
use crate::ui::Ui;

/// Read-only view of the freshly compiled pages handed to every check. Cached
/// pages are excluded by the caller: they kept their links from the build that
/// produced them, so there is nothing new to validate.
pub(super) struct Compiled<'a> {
    pub config: &'a Config,
    pub pages: &'a [CheckedPage<'a>],
    /// The files this build wrote and their sizes, so a page's recorded loads
    /// can be turned into bytes. Absent under `check`, which processes no
    /// assets and so has nothing to weigh them with.
    pub emitted: Option<&'a Emitted>,
}

/// One compiled page's validation-relevant facts. Extend this as new checks need
/// more of the compiled page (its HTML, its dependencies, ...).
pub(super) struct CheckedPage<'a> {
    /// The page's path relative to the content root, for diagnostics.
    pub label: String,
    /// The `.typ` source path, so a check can locate a span within it.
    pub source: &'a Path,
    /// This page's own URL, so a deep link elsewhere can find its anchors.
    pub permalink: &'a str,
    /// Raw targets of the broken internal links this page produced.
    pub broken: &'a [String],
    /// Outbound `http(s)` link targets, empty unless external checking is on.
    pub external: &'a [String],
    /// The heading ids this page exposes.
    pub anchors: &'a [String],
    /// Resolved `"/url/#fragment"` links this page carries into others.
    pub deep: &'a [String],
    /// What the lint pass found while this page rendered.
    pub lints: &'a [Finding],
    /// What this page ships: its inline bytes and the files it loads.
    pub weight: &'a Weight,
    /// The page's own markup, as written to `dist`, for the `html` budget.
    pub html: &'a str,
}

/// Broken internal `.typ` links: every reference must resolve to an existing
/// page, and a reference naming a `#fragment` must find that heading there.
/// Fatal under `links.strict`, otherwise the identical diagnostic as a warning.
pub(super) struct Links;

impl Links {
    /// Links naming a `#fragment` no page exposes.
    ///
    /// A site-wide pass, because a page cannot answer this while it renders:
    /// the headings belong to the *target*, which may not have been compiled
    /// yet. Recomputed from every page's recorded anchors on every build rather
    /// than cached per page, so renaming a heading in B invalidates the verdict
    /// on A without A having changed at all.
    ///
    /// A fragment pointing into the page's own body is checked the same way; the
    /// linking page is simply also the target.
    fn dangling(site: &Compiled) -> Vec<Broken> {
        let anchors: std::collections::HashMap<&str, &[String]> = site
            .pages
            .iter()
            .map(|page| (page.permalink, page.anchors))
            .collect();
        site.pages
            .iter()
            .flat_map(|page| {
                page.deep.iter().filter_map(|target| {
                    let (url, fragment) = target.split_once('#')?;
                    // Only a URL this build produced can be judged. Anything
                    // else is a link out of the site, or into a static file, and
                    // saying "no such heading" about a page baudelaire never
                    // rendered would be a false positive.
                    let ids = anchors.get(url)?;
                    let fragment = fragment.split('?').next().unwrap_or(fragment);
                    if fragment.is_empty() || ids.iter().any(|id| id == fragment) {
                        return None;
                    }
                    Some(Broken::anchor(
                        page.label.clone(),
                        target.clone(),
                        page.source,
                        fragment.to_owned(),
                    ))
                })
            })
            .collect()
    }

    pub(super) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        let broken: Vec<Broken> = site
            .pages
            .iter()
            .flat_map(|page| {
                page.broken
                    .iter()
                    .map(|target| Broken::new(page.label.clone(), target.clone(), page.source))
            })
            .chain(Self::dangling(site))
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

    use miette::Diagnostic as _;

    use super::*;
    use crate::error::BaudelaireErrorKind;
    use crate::ui::Level;

    /// One page carrying the given broken-link targets.
    fn page(broken: &[String]) -> CheckedPage<'_> {
        CheckedPage {
            label: "post.typ".into(),
            source: Path::new("post.typ"),
            permalink: "/post/",
            broken,
            external: &[],
            anchors: &[],
            deep: &[],
            lints: &[],
            weight: Weight::EMPTY,
            html: "",
        }
    }

    /// A page carrying anchors and deep links, for the site-wide fragment check.
    fn deep_page<'a>(
        permalink: &'a str,
        anchors: &'a [String],
        deep: &'a [String],
    ) -> CheckedPage<'a> {
        CheckedPage {
            permalink,
            anchors,
            deep,
            ..page(&[])
        }
    }

    /// A fragment naming a heading the target does not have. This is the case
    /// that used to pass silently: the fragment rode along in the URL and was
    /// never looked at, so renaming a heading broke every link into it.
    #[test]
    fn a_fragment_with_no_matching_heading_is_reported() {
        let config = Config::default();
        let anchors = ["installing".to_owned()];
        let deep = ["/guide/#instaling".to_owned()];
        let pages = [
            deep_page("/guide/", &anchors, &[]),
            deep_page("/", &[], &deep),
        ];
        let found = Links::dangling(&Compiled {
            config: &config,
            pages: &pages,
            emitted: None,
        });
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].code().map(|c| c.to_string()).as_deref(),
            Some("baudelaire::links::anchor")
        );
    }

    /// Everything that must *not* fire: a heading that is there, a link with no
    /// fragment, a fragment into a page this build did not produce (an external
    /// URL, a static file), and a page's link into its own headings.
    #[test]
    fn a_resolvable_or_unjudgeable_fragment_is_left_alone() {
        let config = Config::default();
        let anchors = ["installing".to_owned()];
        let deep = [
            "/guide/#installing".to_owned(),
            "/guide/".to_owned(),
            "/not-a-page-of-ours/#whatever".to_owned(),
        ];
        let pages = [
            deep_page("/guide/", &anchors, &[]),
            deep_page("/", &[], &deep),
        ];
        assert!(
            Links::dangling(&Compiled {
                config: &config,
                pages: &pages,
                emitted: None,
            })
            .is_empty()
        );
    }

    #[test]
    fn links_pass_reports_nothing_when_all_resolve() {
        let config = Config::default();
        let ui = Ui::new(Level::Silent);
        let pages = [page(&[])];
        let site = Compiled {
            config: &config,
            pages: &pages,
            emitted: None,
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
            emitted: None,
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
            emitted: None,
        };

        Links::run(&site, &ui).unwrap();
        assert_eq!(ui.warnings(), 1, "all broken links fold into one warning");
    }
}
