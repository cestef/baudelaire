//! The site-wide half of linting: reporting what the per-page DOM pass found,
//! and weighing each page against its budgets.
//!
//! Both run over every page a build produced, cache hits included, for the same
//! reason [`super::Links`] does: a gate that reports on the first build and
//! stays quiet on the second is not a gate. What the pages recorded is replayed
//! from their cached outputs; what this build emitted is read fresh, so
//! shipping a fatter stylesheet fails the pages that load it without any of
//! them having changed.

use crate::config::BudgetConfig;
use crate::error::{Flaw, Flaws, Overweight, Overweights, Result, Sources};
use crate::render::{Emitted, Load, Weight};
use crate::ui::{Bytes, Ui};

use super::{CheckedPage, Compiled};

/// The findings of the per-page lint pass, gathered into one report. Fatal
/// under `lint { strict }`, otherwise the identical diagnostic as a warning:
/// the same policy [`super::Links`] applies to a broken link, and for the same
/// reason, that whether a finding stops a build belongs to the site.
pub(in crate::engine) struct Lints;

impl Lints {
    pub(in crate::engine) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        let root = &site.config.root;
        let mut sources = Sources::default();
        let flaws: Vec<Flaw> = site
            .pages
            .iter()
            .flat_map(|page| page.lints.iter().map(move |finding| (page, finding)))
            .map(|(page, finding)| {
                let at = finding.at.as_ref();
                Flaw::new(
                    page.label.clone(),
                    finding.lint.clone(),
                    at,
                    sources.at(at, root),
                )
            })
            .collect();
        if flaws.is_empty() {
            return Ok(());
        }
        if site.config.lint.strict {
            return Err(Flaws::new(flaws).into());
        }
        ui.warn(Flaws::warning(flaws));
        Ok(())
    }
}

/// Per-page weight budgets.
///
/// Always fatal, unlike a lint finding: a budget is a limit the author wrote
/// down, and a limit that only warns is a number in a config file.
pub(in crate::engine) struct Budgets;

impl Budgets {
    pub(in crate::engine) fn run(site: &Compiled) -> Result<()> {
        let budget = &site.config.lint.budget;
        // Nothing to weigh against, or nothing to weigh with. The second case
        // is `check`, which processes no assets: it can see what a page loads
        // but not how large any of it is, and a budget checked against zero
        // bytes of stylesheet would pass everything and mean nothing.
        let (Some(emitted), true) = (site.emitted, Self::declared(budget)) else {
            return Ok(());
        };
        let over: Vec<Overweight> = site
            .pages
            .iter()
            .flat_map(|page| Scale::of(page, emitted).against(budget, &page.label))
            .collect();
        match over.is_empty() {
            true => Ok(()),
            false => Err(Overweights::from(over).into()),
        }
    }

    /// Whether the site set any budget at all.
    fn declared(budget: &BudgetConfig) -> bool {
        let BudgetConfig {
            html,
            js,
            css,
            images,
            total,
        } = budget;
        [html, js, css, images, total].iter().any(|b| b.is_some())
    }
}

/// One page on the scales: what each class of its output weighs.
///
/// Inline bytes are held apart from loaded ones because they belong to two
/// answers at once. A page's inline `<script>` counts against `js`, which is
/// about how much JavaScript the visitor runs, *and* it is already inside the
/// markup `html` measures. Adding both into `total` billed those bytes twice:
/// a 1.5 kB page reported 2.5 kB and failed a 2 kB budget it was well inside.
struct Scale {
    /// The page's own markup, inline bodies included.
    html: u64,
    /// Scripts the page loads, and the bytes it inlines.
    js: Class,
    css: Class,
    /// Images are only ever loaded, so this one has no inline half.
    images: u64,
}

/// One class of output, split by where its bytes live.
#[derive(Default)]
struct Class {
    loaded: u64,
    inline: u64,
}

impl Class {
    /// Everything of this kind the visitor gets, wherever it came from: what
    /// the class's own budget is about.
    fn shipped(&self) -> u64 {
        self.loaded + self.inline
    }
}

impl Scale {
    /// Weigh a page: its own markup, its inline bytes, and every file it loads
    /// that this build wrote. A load this build did not write (a CDN URL, a
    /// file from `static/`) weighs nothing here rather than a guess.
    fn of(page: &CheckedPage, emitted: &Emitted) -> Self {
        let Weight { js, css, loads } = page.weight;
        let mut scale = Self {
            html: page.html.len() as u64,
            js: Class {
                inline: *js,
                ..Class::default()
            },
            css: Class {
                inline: *css,
                ..Class::default()
            },
            images: 0,
        };
        for load in loads {
            let Some(bytes) = emitted.at(&load.url) else {
                continue;
            };
            match load.load {
                Load::Js => scale.js.loaded += bytes,
                Load::Css => scale.css.loaded += bytes,
                Load::Image => scale.images += bytes,
            }
        }
        scale
    }

    /// The page's whole transfer weight: the markup, plus everything fetched
    /// alongside it. The inline halves are not added again, being bytes of the
    /// markup already counted.
    fn total(&self) -> u64 {
        self.html + self.js.loaded + self.css.loaded + self.images
    }

    /// Every budget this page breaks. The key is spelled exactly as it is in
    /// the config, so a report names the line to edit.
    fn against(&self, budget: &BudgetConfig, page: &str) -> Vec<Overweight> {
        [
            ("html", self.html, budget.html),
            ("js", self.js.shipped(), budget.js),
            ("css", self.css.shipped(), budget.css),
            ("images", self.images, budget.images),
            ("total", self.total(), budget.total),
        ]
        .into_iter()
        .filter_map(|(name, weighed, allowed)| {
            let allowed = allowed?;
            (weighed > allowed.0).then(|| Overweight {
                page: page.to_owned(),
                budget: name,
                weighed: Bytes(weighed),
                allowed,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::config::Config;
    use crate::render::{Finding, Reference};

    /// A page carrying one weight ledger and the given markup.
    fn page<'a>(html: &'a str, weight: &'a Weight, lints: &'a [Finding]) -> CheckedPage<'a> {
        CheckedPage {
            label: "post.typ".into(),
            source: Path::new("post.typ"),
            permalink: "/post/",
            broken: &[],
            external: &[],
            anchors: &[],
            deep: &[],
            lints,
            weight,
            html,
        }
    }

    fn emitted() -> Emitted {
        let mut emitted = Emitted::new(String::new());
        emitted.insert("/assets/app.js".to_owned(), 900);
        emitted
    }

    fn config(text: &str) -> Config {
        Config::parse(text).expect("should parse")
    }

    /// A loaded script counts against `js`, and so does an inline one.
    #[test]
    fn a_page_is_weighed_by_what_it_loads_and_what_it_inlines() {
        let weight = Weight {
            js: 100,
            css: 0,
            loads: vec![Reference {
                load: Load::Js,
                url: "/assets/app.js".into(),
            }],
        };
        let scale = Scale::of(&page("<p>hi</p>", &weight, &[]), &emitted());
        assert_eq!(scale.js.shipped(), 1000);
        assert_eq!(scale.html, 9);
        // ...and the inline hundred is *not* added to the total a second time:
        // those bytes are part of the nine-byte markup already counted.
        assert_eq!(scale.total(), 909);
    }

    /// The report names the budget that broke, and only that one.
    #[test]
    fn only_the_budgets_a_page_breaks_are_reported() {
        let weight = Weight::default();
        let scale = Scale::of(&page("0123456789", &weight, &[]), &emitted());
        let budget = config("lint { budget { html 5; total \"1kB\" } }")
            .lint
            .budget;
        let over = scale.against(&budget, "post.typ");
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].budget, "html");
        assert_eq!(over[0].weighed, Bytes(10));
    }

    /// With no asset sizes to hand there is nothing honest to say, so a build
    /// that cannot weigh (`check`) reports nothing rather than passing every
    /// page against a stylesheet it measured as zero bytes.
    #[test]
    fn a_build_with_nothing_emitted_weighs_nothing() {
        let config = config("lint { budget { html 1 } }");
        let weight = Weight::default();
        let pages = [page("far too long for one byte", &weight, &[])];
        let site = Compiled {
            config: &config,
            pages: &pages,
            emitted: None,
        };
        Budgets::run(&site).unwrap();
    }

    /// The same site, weighed: the page is over and the build fails.
    #[test]
    fn an_oversized_page_fails_the_build() {
        let config = config("lint { budget { html 1 } }");
        let emitted = emitted();
        let weight = Weight::default();
        let pages = [page("far too long for one byte", &weight, &[])];
        let site = Compiled {
            config: &config,
            pages: &pages,
            emitted: Some(&emitted),
        };
        assert!(Budgets::run(&site).is_err());
    }

    /// A site with no budget declared never weighs anything, however much it
    /// ships.
    #[test]
    fn a_site_with_no_budget_is_never_over_one() {
        let config = config("lint { }");
        let emitted = emitted();
        let weight = Weight::default();
        let pages = [page("anything at all", &weight, &[])];
        let site = Compiled {
            config: &config,
            pages: &pages,
            emitted: Some(&emitted),
        };
        Budgets::run(&site).unwrap();
    }

    /// Strictness decides whether a finding stops the build, exactly as it does
    /// for a broken link.
    #[test]
    fn findings_warn_or_fail_by_the_strict_gate() {
        let lints = [Finding {
            lint: crate::error::Lint::Alt,
            at: None,
        }];
        let weight = Weight::default();
        let pages = [page("<img>", &weight, &lints)];
        let lenient = config("lint { }");
        let ui = Ui::new(crate::ui::Level::Silent);
        Lints::run(
            &Compiled {
                config: &lenient,
                pages: &pages,
                emitted: None,
            },
            &ui,
        )
        .unwrap();
        assert_eq!(ui.warnings(), 1, "all findings fold into one warning");

        let strict = config("lint { strict }");
        assert!(
            Lints::run(
                &Compiled {
                    config: &strict,
                    pages: &pages,
                    emitted: None,
                },
                &ui,
            )
            .is_err()
        );
    }
}
