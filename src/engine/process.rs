//! Post-build processors: whole-site passes that emit derived files.
//!
//! Each [`Processor`] reads the built [`Site`] and writes derived output
//! through an [`Emit`] sink. [`Processors::builtin`] is the single source of
//! what runs, in order: a new site-level output (search index, robots.txt) is
//! one `impl Processor` plus one line in that list.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::error::warning::BaseUrlMissing;
use crate::error::{BaseUrlRequired, Result};
use crate::ui::Ui;

use super::feed::Feeds;
use super::llms::Llms;
use super::redirect::Redirects;
use super::robots::Robots;
use super::search::SearchIndex;
use super::sitemap::SiteMap;
use super::standard::WellKnown;

/// Read-only view of the fully built site handed to every processor.
pub(super) struct Site<'a> {
    pub config: &'a Config,
    pub pages: &'a [Page],
    /// Each page paired with its rendered HTML (cached and freshly compiled
    /// alike), for processors that derive from page text, e.g. search.
    pub outputs: &'a [(&'a Page, &'a str)],
}

impl Site<'_> {
    /// The base URL a processor cannot work without. An error, not a warning:
    /// these features are opt-in, so reaching here means the site asked for
    /// output that cannot be produced, and warning let CI go green with no feed.
    pub(super) fn base(&self, feature: &'static str) -> Result<BaseUrl> {
        self.config
            .base()
            .ok_or_else(|| BaseUrlRequired { feature }.into())
    }

    /// The base URL, warning with `missing` when absent. The single "is a `url`
    /// configured?" check shared by every processor: skip-on-absent callers go
    /// through [`Site::base`]; those that still emit (llms with relative links,
    /// robots dropping its sitemap line) supply their own consequence here.
    pub(super) fn warn_missing_base(
        &self,
        out: &mut dyn Emit,
        missing: BaseUrlMissing,
    ) -> Result<Option<BaseUrl>> {
        let base = self.config.base();
        if base.is_none() {
            out.warn(missing);
        }
        Ok(base)
    }
}

/// Sink for a processor's output: file writes plus progress reporting.
///
/// A trait so processors are unit-testable against an in-memory sink instead of
/// the real filesystem. [`Emit::file`] is silent by design: the processor
/// decides what to report via [`Emit::note`], matching the per-feature phrasing
/// the CLI already uses.
pub(super) trait Emit {
    /// Write `contents` to absolute `path`, creating parent directories.
    fn file(&mut self, path: &Path, contents: &str) -> Result<()>;
    /// A progress note (e.g. `wrote sitemap.xml`): a debug log line in
    /// production, captured verbatim by test sinks.
    fn note(&mut self, msg: fmt::Arguments);
    /// A warning from a processor, already boxed. Call [`Warn::warn`] instead:
    /// this is the object-safe primitive it forwards to, the same split [`Ui`]
    /// makes between `warn` and `report`.
    fn report(&mut self, warning: Box<dyn miette::Diagnostic + Send + Sync>);
}

/// Typed `warn` over any [`Emit`], so a processor names the diagnostic it is
/// raising instead of boxing at the call site. Blanket, so every sink gets it.
pub(super) trait Warn {
    fn warn(&mut self, warning: impl miette::Diagnostic + Send + Sync + 'static);
}

impl<T: Emit + ?Sized> Warn for T {
    fn warn(&mut self, warning: impl miette::Diagnostic + Send + Sync + 'static) {
        self.report(Box::new(warning));
    }
}

/// One post-build pass over the site.
pub(super) trait Processor {
    /// Whether to run, from config alone: keeps the gate declarative and out of
    /// [`Processor::run`]. Default: always.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    /// Emit output derived from the site. Called only when [`Processor::enabled`].
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()>;
}

/// The built-in processors, in run order. THE single source of what runs
/// post-build: add a site-level output by adding one line here.
pub(super) struct Processors(Vec<Box<dyn Processor>>);

impl Processors {
    pub(super) fn builtin() -> Self {
        Self(vec![
            Box::new(Redirects),
            Box::new(SiteMap),
            Box::new(Robots),
            Box::new(Llms),
            Box::new(Feeds),
            Box::new(SearchIndex),
            Box::new(WellKnown),
        ])
    }

    /// Run each enabled processor in order; the first error stops the build.
    pub(super) fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        for processor in &self.0 {
            if processor.enabled(site.config) {
                processor.run(site, out)?;
            }
        }
        Ok(())
    }
}

/// The production [`Emit`] sink: writes through [`crate::fs`], logs notes as
/// debug events, and collects warnings on the shared [`Ui`].
pub(super) struct Emitter<'a> {
    ui: &'a Ui,
    bytes: u64,
    /// Every generated file written this build, so the prune pass keeps them.
    paths: Vec<PathBuf>,
    /// Destinations the static tree already owns. A processor never overwrites
    /// one: `static/` is documented as the override escape hatch, yet
    /// processors run *after* the static copy, so with `sitemap` on by default a
    /// hand-authored `static/sitemap.xml` was clobbered on every build, out of
    /// the box. Pages still win over static; only these whole-site derived
    /// files yield.
    reserved: BTreeSet<PathBuf>,
}

impl<'a> Emitter<'a> {
    pub(super) fn new(ui: &'a Ui, reserved: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            ui,
            bytes: 0,
            paths: Vec::new(),
            reserved: reserved.into_iter().collect(),
        }
    }

    /// How many files were written: the count of generated outputs for the
    /// build summary (feeds, sitemap, search index, and so on).
    pub(super) fn written(&self) -> usize {
        self.paths.len()
    }

    /// Total bytes of generated output written, for the build summary.
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The generated files written this build, for the prune pass.
    pub(super) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

impl Emit for Emitter<'_> {
    fn file(&mut self, path: &Path, contents: &str) -> Result<()> {
        if self.reserved.contains(path) {
            tracing::debug!(path = %path.display(), "kept the static file over generated output");
            return Ok(());
        }
        crate::fs::write_all(path, contents)?;
        self.bytes += contents.len() as u64;
        self.paths.push(path.to_path_buf());
        Ok(())
    }

    fn note(&mut self, msg: fmt::Arguments) {
        tracing::debug!("{msg}");
    }

    fn report(&mut self, warning: Box<dyn miette::Diagnostic + Send + Sync>) {
        self.ui.report(warning);
    }
}

/// In-memory [`Emit`] sink capturing everything a processor emits. Lives at
/// module scope so every processor's own tests share one sink instead of each
/// re-declaring it.
#[cfg(test)]
#[derive(Default)]
pub(super) struct Recorder {
    pub files: Vec<(PathBuf, String)>,
    pub notes: Vec<String>,
    pub warns: Vec<String>,
}

#[cfg(test)]
impl Emit for Recorder {
    fn file(&mut self, path: &Path, contents: &str) -> Result<()> {
        self.files.push((path.to_path_buf(), contents.to_owned()));
        Ok(())
    }

    fn note(&mut self, msg: fmt::Arguments) {
        self.notes.push(msg.to_string());
    }

    fn report(&mut self, warning: Box<dyn miette::Diagnostic + Send + Sync>) {
        self.warns.push(warning.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A processor that records its label when it runs, gated by a fixed flag.
    struct Marker(&'static str, bool);

    impl Processor for Marker {
        fn enabled(&self, _config: &Config) -> bool {
            self.1
        }

        fn run(&self, _site: &Site, out: &mut dyn Emit) -> Result<()> {
            out.note(format_args!("ran {}", self.0));
            Ok(())
        }
    }

    #[test]
    fn registry_runs_only_enabled_processors_in_order() {
        let config = Config::default();
        let site = Site {
            config: &config,
            pages: &[],
            outputs: &[],
        };
        let registry = Processors(vec![
            Box::new(Marker("first", true)),
            Box::new(Marker("skipped", false)),
            Box::new(Marker("last", true)),
        ]);

        let mut rec = Recorder::default();
        registry.run(&site, &mut rec).unwrap();

        assert_eq!(rec.notes, ["ran first", "ran last"]);
        assert!(rec.warns.is_empty());
    }
}
