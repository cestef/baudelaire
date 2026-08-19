//! Post-build processors: whole-site passes that emit derived files, one
//! [`Processor`] per module, run in the order [`Processors::builtin`] lists.

mod csp;
#[cfg(feature = "js")]
pub(crate) mod dts;
#[cfg(feature = "epub")]
mod epub;
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
#[cfg(feature = "epub")]
mod xhtml;
mod xml;

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::error::warning::BaseUrlMissing;
use crate::error::{Artifact, BaseUrlRequired, Result, SerializeError};
use crate::render::{Fragments, Syndicated};
use crate::ui::Ui;

/// One built page: everything the render pass produced for it, whether it was
/// freshly compiled or served from the cache.
pub(super) struct Output<'a> {
    pub page: &'a Page,
    /// The page's rendered HTML, exactly as written to `dist`.
    pub html: &'a str,
    /// Its head and body markup, present only while the single-file export is
    /// on.
    pub fragments: Option<&'a Fragments>,
    /// Its prose as a feed publishes it, present only while
    /// `generate { feed { content "full" } }` is on.
    pub syndicated: Option<&'a Syndicated>,
    /// The digests of its inline scripts and styles, for the generated content
    /// security policy. Empty unless one is being generated.
    pub inline: &'a crate::render::Inline,
}

#[cfg(test)]
impl<'a> Output<'a> {
    pub(super) fn new(page: &'a Page, html: &'a str) -> Self {
        Self {
            page,
            html,
            fragments: None,
            syndicated: None,
            inline: crate::render::Inline::EMPTY,
        }
    }
}

/// Read-only view of the fully built site handed to every processor.
pub(super) struct Site<'a> {
    pub config: &'a Config,
    pub pages: &'a [Page],
    /// The entity registries, so a feed entry names the people the page
    /// credits rather than the site's one `author`.
    pub entities: &'a crate::content::Registries,
    /// Every built page, cached and freshly compiled alike.
    pub outputs: &'a [Output<'a>],
}

impl Site<'_> {
    /// Where a generated file goes: `segments` under this build's `dist`.
    ///
    /// An empty segment contributes nothing, so a caller passes a language
    /// scope (`""` for the default language) without first deciding whether
    /// there is one.
    pub(super) fn dist(&self, segments: &[&str]) -> PathBuf {
        let mut path = self.config.paths.dist.clone();
        path.extend(segments.iter().copied().filter(|s| !s.is_empty()));
        path
    }

    /// The base URL a processor cannot work without. An error, not a warning:
    /// these features are opt-in, so reaching here means the site asked for
    /// output that cannot be produced.
    pub(super) fn base(&self, feature: &'static str) -> Result<BaseUrl> {
        self.config
            .base()
            .ok_or_else(|| BaseUrlRequired { feature }.into())
    }

    /// The base URL, warning with `missing` when absent, for a processor that
    /// emits anyway; one that cannot goes through [`Site::base`] instead.
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
    /// This artifact serialized to JSON, naming itself on failure.
    pub(super) fn json<T: Serialize>(self, value: &T) -> Result<String> {
        serde_json::to_string(value).map_err(|e| SerializeError::new(self, e).into())
    }
}

const WROTE: &str = "wrote";

/// Sink for a processor's output: file writes plus progress reporting.
///
/// [`Emit::file`] is silent by design: the processor decides what to report via
/// [`Emit::note`].
pub(super) trait Emit {
    /// Write `contents` to absolute `path`, creating parent directories.
    fn file(&mut self, path: &Path, contents: &str) -> Result<()>;
    /// The same for output that is not text, an EPUB's zip among them.
    fn binary(&mut self, path: &Path, contents: &[u8]) -> Result<()>;
    /// Whether a static file already claims `path`, so [`file`](Emit::file)
    /// would keep that one and drop what a processor writes.
    ///
    /// Asked rather than discovered, because the drop is silent and a processor
    /// whose *only* output is shadowed has to do something else instead.
    fn claimed(&self, path: &Path) -> bool;
    /// A progress note (e.g. `wrote 3 redirects`): a debug log line in
    /// production, captured verbatim by test sinks. Prefer [`Emit::wrote`].
    fn note(&mut self, msg: fmt::Arguments);

    /// Note that `path` was written.
    fn wrote(&mut self, path: &Path) {
        self.note(format_args!("{WROTE} {}", path.display()));
    }

    /// The same note with something only that processor knows after it, as
    /// `wrote <path> (<detail>)`.
    fn wrote_with(&mut self, path: &Path, detail: fmt::Arguments) {
        self.note(format_args!("{WROTE} {} ({detail})", path.display()));
    }

    /// A warning from a processor, already boxed: the object-safe primitive
    /// [`Warn::warn`] forwards to.
    fn report(&mut self, warning: Box<dyn miette::Diagnostic + Send + Sync>);
}

/// Typed `warn` over any [`Emit`], so a processor names the diagnostic it is
/// raising instead of boxing at the call site.
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
    /// Whether to run, from config alone. Default: always.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    /// Emit output derived from the site, only when [`Processor::enabled`].
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()>;
}

/// The built-in processors, in run order.
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
            #[cfg(feature = "epub")]
            Box::new(epub::Epub),
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
    /// Destinations the static tree already owns; a processor never overwrites
    /// one, since `static/` is the override escape hatch and processors run
    /// after the static copy.
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

    pub(super) fn written(&self) -> usize {
        self.paths.len()
    }

    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(super) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

impl Emit for Emitter<'_> {
    fn claimed(&self, path: &Path) -> bool {
        self.reserved.contains(path)
    }

    fn file(&mut self, path: &Path, contents: &str) -> Result<()> {
        self.binary(path, contents.as_bytes())
    }

    fn binary(&mut self, path: &Path, contents: &[u8]) -> Result<()> {
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

/// In-memory [`Emit`] sink capturing everything a processor emits.
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

    /// Recorded by *size*, not content: a test asserting on a zip's bytes would
    /// be asserting on the compressor.
    fn binary(&mut self, path: &Path, contents: &[u8]) -> Result<()> {
        self.files
            .push((path.to_path_buf(), format!("<{} bytes>", contents.len())));
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
            entities: crate::content::Registries::none(),
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
