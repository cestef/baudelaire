//! The site-wide half of linting: reporting what the per-page DOM pass found,
//! and weighing each page against its budgets. Both run over every page a build
//! produced, cache hits included, or a gate reports only on the first build.

use crate::config::{BudgetConfig, Severity};
use crate::error::{Flaw, Flaws, Overweight, Overweights, Result, Sources};
use crate::render::{Emitted, Load, Weight};
use crate::ui::{Bytes, Ui};

use super::{CheckedPage, Compiled};

/// The findings of the per-page lint pass, gathered into one report. Fatal
/// under `lint { strict }`, otherwise the identical diagnostic as a warning.
pub(in crate::engine) struct Lints;

impl Lints {
    pub(in crate::engine) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        let root = &site.config.root;
        let mut sources = Sources::default();
        let flaws: Vec<(Severity, Flaw)> = site
            .pages
            .iter()
            .flat_map(|page| page.lints.iter().map(move |finding| (page, finding)))
            .map(|(page, finding)| {
                let at = finding.at.as_ref();
                let flaw = Flaw::new(
                    page.label.clone(),
                    finding.lint.clone(),
                    at,
                    sources.at(at, root),
                );
                (site.config.lint.severity(&finding.lint.ruled()), flaw)
            })
            .collect();
        if flaws.is_empty() {
            return Ok(());
        }
        let (fatal, warned): (Vec<_>, Vec<_>) = flaws
            .into_iter()
            .partition(|(severity, _)| *severity == Severity::Error);
        let unwrap = |v: Vec<(Severity, Flaw)>| v.into_iter().map(|(_, flaw)| flaw).collect();
        let warned: Vec<Flaw> = unwrap(warned);
        if !warned.is_empty() {
            ui.warn(Flaws::warning(warned));
        }
        if fatal.is_empty() {
            Ok(())
        } else {
            Err(Flaws::new(unwrap(fatal)).into())
        }
    }
}

/// Per-page weight budgets.
pub(in crate::engine) struct Budgets;

impl Budgets {
    pub(in crate::engine) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        let budget = &site.config.lint.budget;
        let (Some(emitted), true) = (site.emitted, Self::declared(budget)) else {
            return Ok(());
        };
        let over: Vec<Overweight> = site
            .pages
            .iter()
            .flat_map(|page| Scale::of(page, emitted).against(budget, &page.label))
            .collect();
        if over.is_empty() {
            return Ok(());
        }
        if !budget.strict {
            ui.warn(Overweights::warning(over));
            return Ok(());
        }
        Err(Overweights::new(over).into())
    }

    /// Whether the site set any budget at all.
    fn declared(budget: &BudgetConfig) -> bool {
        let BudgetConfig {
            strict: _,
            html,
            js,
            css,
            images,
            total,
        } = budget;
        [html, js, css, images, total].iter().any(|b| b.is_some())
    }
}

/// One page on the scales: what each class of its output weighs. Inline bytes
/// are held apart from loaded ones because they count against their class *and*
/// sit inside the markup `html` measures, so `total` must not bill them twice.
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
    /// Everything of this kind the visitor gets, wherever it came from.
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
            let Some(bytes) = emitted.at(&load.url).map(|file| file.bytes) else {
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

    /// Every budget this page breaks, keyed as the config spells it.
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

    fn page<'a>(html: &'a str, weight: &'a Weight, lints: &'a [Finding]) -> CheckedPage<'a> {
        CheckedPage {
            outbound: crate::render::Outbound::EMPTY,
            lists: &[],
            generated: false,
            listed: true,
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
        emitted.insert("/assets/app.js".to_owned(), &[b'x'; 900], false);
        emitted
    }

    fn config(text: &str) -> Config {
        Config::parse(text).expect("should parse")
    }

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
        assert_eq!(scale.total(), 909);
    }

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
        Budgets::run(&site, &Ui::new(crate::ui::Level::Silent)).unwrap();
    }

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
        assert!(Budgets::run(&site, &Ui::new(crate::ui::Level::Silent)).is_err());
    }

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
        Budgets::run(&site, &Ui::new(crate::ui::Level::Silent)).unwrap();
    }

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
