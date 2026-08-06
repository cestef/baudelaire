//! Post-build processors: whole-site passes that emit derived files.
//!
//! Each [`Processor`] reads the built [`Site`] and writes derived output
//! through an [`Emit`] sink. [`Processors::builtin`] is the single source of
//! what runs, in order: a new site-level output (search index, robots.txt) is
//! one `impl Processor` in a module of its own, plus one line in that list.
//! Processors are named through their module there rather than imported, so
//! adding one touches nothing else here and the list doubles as the map of
//! which module emits what.

mod csp;
#[cfg(feature = "js")]
pub(crate) mod dts;
mod feed;
mod headers;
mod line;
mod llms;
mod manifest;
mod redirect;
mod robots;
mod script;
mod search;
mod sitemap;
mod spa;
mod standalone;
#[cfg(feature = "announce")]
mod standard;
mod xml;

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::error::warning::BaseUrlMissing;
use crate::error::{Artifact, BaseUrlRequired, Result, SerializeError};
use crate::render::Fragments;
use crate::ui::Ui;

/// One built page: everything the render pass produced for it, whether it was
/// freshly compiled or served from the cache. Named rather than a tuple because
/// processors read different parts of it (search wants the text, the
/// single-file export wants the fragments) and a widening tuple made every call
/// site restate the ones it ignores.
pub(super) struct Output<'a> {
    pub page: &'a Page,
    /// The page's rendered HTML, exactly as written to `dist`.
    pub html: &'a str,
    /// Its head and body markup, present only while the single-file export is
    /// on (nothing else pays to capture them).
    pub fragments: Option<&'a Fragments>,
    /// The digests of its inline scripts and styles, for the generated content
    /// security policy. Empty unless one is being generated.
    pub inline: &'a crate::render::Inline,
}

#[cfg(test)]
impl<'a> Output<'a> {
    /// A page and its markup, with no fragments: what a test builds when the
    /// processor under it never looks at the single-file export's half.
    pub(super) fn new(page: &'a Page, html: &'a str) -> Self {
        Self {
            page,
            html,
            fragments: None,
            inline: crate::render::Inline::EMPTY,
        }
    }
}

/// Read-only view of the fully built site handed to every processor.
pub(super) struct Site<'a> {
    pub config: &'a Config,
    pub pages: &'a [Page],
    /// Every built page (cached and freshly compiled alike), for processors
    /// that derive from what the render pass produced.
    pub outputs: &'a [Output<'a>],
}

impl Site<'_> {
    /// Where a generated file goes: `segments` under this build's `dist`.
    ///
    /// The single output-path rule for every processor, so none of them reaches
    /// into `config.paths` itself. An empty segment contributes nothing, so a
    /// caller passes a language scope (`""` for the default language) without
    /// first deciding whether there is one.
    pub(super) fn dist(&self, segments: &[&str]) -> PathBuf {
        let mut path = self.config.paths.dist.clone();
        path.extend(segments.iter().copied().filter(|s| !s.is_empty()));
        path
    }

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
    ) -> Option<BaseUrl> {
        let base = self.config.base();
        if base.is_none() {
            out.warn(missing);
        }
        base
    }
}

impl Artifact {
    /// This artifact serialized to JSON, naming itself on failure. The one
    /// serialize-and-tag step shared by the JSON feed, the search indexes, and
    /// the single-file export's route table, so none of them restates which
    /// artifact a `serde_json` failure belongs to.
    pub(super) fn json<T: Serialize>(self, value: &T) -> Result<String> {
        serde_json::to_string(value).map_err(|e| SerializeError::new(self, e).into())
    }
}

/// The verb every progress note opens with. A const rather than a literal in
/// each of the three notes that spell it, since it is the one word a reader
/// greps a verbose build for.
const WROTE: &str = "wrote";

/// Sink for a processor's output: file writes plus progress reporting.
///
/// A trait so processors are unit-testable against an in-memory sink instead of
/// the real filesystem. [`Emit::file`] is silent by design: the processor
/// decides what to report via [`Emit::note`], matching the per-feature phrasing
/// the CLI already uses.
pub(super) trait Emit {
    /// Write `contents` to absolute `path`, creating parent directories.
    fn file(&mut self, path: &Path, contents: &str) -> Result<()>;
    /// Whether a static file already claims `path`, so [`file`](Emit::file)
    /// would keep that one and drop what a processor writes.
    ///
    /// Asked rather than discovered, because the drop is silent by design (the
    /// static tree is the escape hatch, and it wins) and a processor whose
    /// *only* output is shadowed has to do something else instead of nothing.
    fn claimed(&self, path: &Path) -> bool;
    /// A progress note (e.g. `wrote 3 redirects`): a debug log line in
    /// production, captured verbatim by test sinks. Prefer [`Emit::wrote`],
    /// which is what nearly every processor has to say.
    fn note(&mut self, msg: fmt::Arguments);

    /// Note that `path` was written.
    ///
    /// The one spelling of the note almost every processor ends with. Half of
    /// them named the file (`wrote robots.txt`) and half the destination it
    /// landed at, so one build's log answered "and where did that go?" two
    /// different ways depending on which processor was speaking. The
    /// destination is the answer, since it is the thing a reader can go and
    /// look at.
    fn wrote(&mut self, path: &Path) {
        self.note(format_args!("{WROTE} {}", path.display()));
    }

    /// The same note with something only that processor knows after it, as
    /// `wrote <path> (<detail>)`: how many documents an index holds, how many
    /// routes an export carries.
    fn wrote_with(&mut self, path: &Path, detail: fmt::Arguments) {
        self.note(format_args!("{WROTE} {} ({detail})", path.display()));
    }

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
            Box::new(redirect::Redirects),
            Box::new(sitemap::SiteMap),
            Box::new(robots::Robots),
            Box::new(headers::Headers),
            Box::new(llms::Llms),
            Box::new(manifest::WebManifest),
            Box::new(feed::Feeds),
            Box::new(search::SearchIndex),
            #[cfg(feature = "announce")]
            Box::new(standard::WellKnown),
            Box::new(spa::Spa),
            // last: it reads every other page's markup, not what they emit
            Box::new(standalone::Standalone),
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
    fn claimed(&self, path: &Path) -> bool {
        self.reserved.contains(path)
    }

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
    fn claimed(&self, _path: &Path) -> bool {
        false
    }

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
