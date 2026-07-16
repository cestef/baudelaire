//! Post-build processors: whole-site passes that emit derived files.
//!
//! Each [`Processor`] reads the built [`Site`] and writes derived output
//! through an [`Emit`] sink. [`Processors::builtin`] is the single source of
//! what runs, in order: a new site-level output (search index, robots.txt) is
//! one `impl Processor` plus one line in that list.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::error::Result;
use crate::error::warning::BaseUrlMissing;
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
    /// The base URL for a URL-requiring processor: the one warn-and-skip
    /// policy for a missing `url`, naming the `feature` that needs it.
    pub(super) fn base(
        &self,
        feature: &'static str,
        out: &mut dyn Emit,
    ) -> Result<Option<BaseUrl>> {
        self.warn_missing_base(
            out,
            BaseUrlMissing {
                feature,
                effect: "skipped",
            },
        )
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
    /// A typed warning: an enabled feature missing its `url` precondition.
    fn warn(&mut self, warning: BaseUrlMissing);
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
}

impl<'a> Emitter<'a> {
    pub(super) fn new(ui: &'a Ui) -> Self {
        Self {
            ui,
            bytes: 0,
            paths: Vec::new(),
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
        crate::fs::write_all(path, contents)?;
        self.bytes += contents.len() as u64;
        self.paths.push(path.to_path_buf());
        Ok(())
    }

    fn note(&mut self, msg: fmt::Arguments) {
        tracing::debug!("{msg}");
    }

    fn warn(&mut self, warning: BaseUrlMissing) {
        self.ui.warn(warning);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// In-memory [`Emit`] sink capturing everything a processor emits.
    #[derive(Default)]
    struct Recorder {
        files: Vec<(PathBuf, String)>,
        notes: Vec<String>,
        warns: Vec<String>,
    }

    impl Emit for Recorder {
        fn file(&mut self, path: &Path, contents: &str) -> Result<()> {
            self.files.push((path.to_path_buf(), contents.to_owned()));
            Ok(())
        }

        fn note(&mut self, msg: fmt::Arguments) {
            self.notes.push(msg.to_string());
        }

        fn warn(&mut self, warning: BaseUrlMissing) {
            self.warns.push(warning.to_string());
        }
    }

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
