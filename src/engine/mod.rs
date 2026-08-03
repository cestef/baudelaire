//! Build pipeline: discover -> compile -> render -> write, parallelized via rayon.

pub(crate) mod asset;
mod check;
mod compile;
mod emit;
mod hook;
mod layers;
mod prune;
mod statics;
mod summary;
pub mod text;

use std::path::PathBuf;
use std::sync::Arc;

use rayon::prelude::*;
use tracing::debug;
use typst::syntax::{FileId, Source};
use typst_html::{HtmlDocument, HtmlOptions};

use crate::codegen::Value;
use crate::config::{Config, SearchConfig};
use crate::content::{Page, plan};
use crate::engine::asset::Assets;
#[cfg(feature = "js")]
use crate::engine::asset::JsCtx;
use crate::engine::check::External;
use crate::engine::check::{Budgets, CheckedPage, Compiled, Links, Lints};
#[cfg(feature = "pdf")]
use crate::engine::compile::bundle::Bundle;
use crate::engine::compile::image::Images;
use crate::engine::compile::prepare::{Prepare, Prepared};
use crate::engine::compile::sidecar::{Artifact, Sidecars, Tally};
use crate::engine::emit::{Emitter, Output, Processors, Site};
use crate::engine::hook::Hooks;
use crate::engine::statics::{Copied, Static};
use crate::engine::summary::Summary;
use crate::error::warning::{FeatureMissing, SettingInert};
use crate::error::{BaudelaireErrorKind, BuildFailed, ConfigError, Result, TypstSourceDiagnostic};
use crate::fs;
use crate::graph::{
    Analyzer, Cache, Deps, Hash, Outputs, Reads, Recorded, RenderInputs, Root, Roots,
};
use crate::render::{
    AssetDeps, AssetMap, Emitted, Fragments, LinkDeps, Renderer, SrcSetDeps, SrcSets,
};
use crate::theme::Theme;
use crate::ui::{Count, Dur, PageStatus, Timer, Ui};
pub use crate::world::Mode;
use crate::world::{PageWorld, Project, Tracked};

/// Build statistics returned to callers (the dev server renders its own concise
/// line from these; the CLI prints the full [`Summary`]).
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub pages: usize,
    pub cached: usize,
}

/// The bundled documents a build dealt with: the ones it exported, and every
/// one the site asks for.
///
/// The paths are derived from the config and the page set, never from what was
/// written, for the same reason a sidecar's are: a cached bundle produces no
/// artifact, and a keep-set built from what this build wrote would sweep it.
#[derive(Default)]
struct Bundled {
    drawn: Vec<Artifact>,
    paths: Vec<PathBuf>,
}

/// What the post-build processors emitted, for the summary and the prune.
struct Generated {
    count: usize,
    bytes: u64,
    paths: Vec<PathBuf>,
}

/// One optional capability, the config that asks for it, and what a binary
/// built without it does instead.
///
/// The single source of truth for feature degradation. Every `#[cfg(feature)]`
/// in the tree removes capability in silence: a `.css` file is copied verbatim,
/// a card is never drawn, an image is never re-encoded, and the build is green
/// either way. One row here is what turns that into a diagnostic, instead of a
/// warning hand-written at each site that happens to notice.
struct Gate {
    /// The cargo feature that compiles the capability in.
    cargo: &'static str,
    /// Whether this binary has it. Spelled per row because `cfg!` takes a
    /// feature name literally and so cannot be derived from `cargo`.
    compiled: bool,
    /// The config that asks for it, as the author writes it in `config.kdl`.
    setting: &'static str,
    /// Whether this site asked.
    asked: fn(&Config) -> bool,
    /// What the build produces instead.
    effect: &'static str,
    /// Whether this capability is what rewrites the references *inside* the
    /// files it owns. Content-hashing renames files that other files name, so a
    /// build that lost such a rewriter serves a stylesheet still naming its
    /// assets by their pre-hash spelling: 404s out of a green build. Losing one
    /// turns `assets { fingerprint }` off for the whole build instead.
    rewrites: bool,
}

const GATES: &[Gate] = &[
    Gate {
        cargo: "css",
        compiled: cfg!(feature = "css"),
        setting: "assets { minify }",
        asked: |config| config.assets.minify,
        effect: "stylesheets are copied unminified",
        rewrites: false,
    },
    Gate {
        cargo: "css",
        compiled: cfg!(feature = "css"),
        setting: "assets { fingerprint }",
        asked: |config| config.assets.fingerprint,
        effect: "asset filenames are left unhashed, since the `url()` and `@import` references inside stylesheets cannot be rewritten to match",
        rewrites: true,
    },
    Gate {
        cargo: "js",
        compiled: cfg!(feature = "js"),
        setting: "assets { bundle }",
        asked: |config| config.assets.bundle,
        effect: "JavaScript is copied verbatim, its imports unresolved and its output unminified",
        rewrites: false,
    },
    Gate {
        cargo: "images",
        compiled: cfg!(feature = "images"),
        setting: "assets { images { optimize } }",
        asked: |config| config.assets.images.optimize.any(),
        effect: "PNG and JPEG assets are copied unoptimized",
        rewrites: false,
    },
    Gate {
        cargo: "images",
        compiled: cfg!(feature = "images"),
        setting: "assets { images { responsive } }",
        asked: |config| config.assets.images.responsive.enabled,
        effect: "no width variants are written and no `srcset` is emitted",
        rewrites: false,
    },
    Gate {
        cargo: "cards",
        compiled: cfg!(feature = "cards"),
        setting: "generate { cards }",
        asked: |config| config.generate.cards.enabled,
        effect: "no social card is rendered",
        rewrites: false,
    },
    Gate {
        cargo: "pdf",
        compiled: cfg!(feature = "pdf"),
        setting: "generate { pdf }",
        asked: |config| config.generate.pdf.enabled(),
        effect: "no PDF is written beside a page, and nothing links to one",
        rewrites: false,
    },
    // Unlike its neighbours this names a capability of `deploy`, not of the
    // build. It sits here anyway so the table stays the single place a gated
    // capability is declared, and so a build warns about a destination it will
    // not be able to reach rather than waiting for the deploy to say so.
    Gate {
        cargo: "ssh",
        compiled: cfg!(feature = "ssh"),
        setting: "deploy { ssh }",
        asked: |config| config.deploy.ssh.is_some(),
        effect: "the SSH destination is skipped",
        rewrites: false,
    },
    // Announcing is a command, but it also shapes the *build*: a pinned `did`
    // emits a `.well-known` record and a per-page backlink. Both vanish here,
    // which a site that pinned a `did` very much wants to hear about, since
    // their absence is what makes a publication unverifiable.
    Gate {
        cargo: "announce",
        compiled: cfg!(feature = "announce"),
        setting: "announce { standard }",
        asked: |config| config.announce.standard.is_some(),
        effect: "no verification artifacts are emitted and `announce` is unavailable",
        rewrites: false,
    },
];

/// One config setting that does nothing unless another is also set.
///
/// The counterpart of [`Gate`] for settings gated by *each other* rather than
/// by a cargo feature, and the single source of truth for that class the same
/// way. Each of these was accepted by the parser, changed nothing about the
/// build, and said nothing: a `stopwords` list tuning an index format the site
/// does not emit, a `terms` feed over taxonomies that publish no term page.
struct Inert {
    /// The setting that was asked for, as the author writes it in `config.kdl`.
    setting: &'static str,
    /// Whether this site asked.
    asked: fn(&Config) -> bool,
    /// What it depends on.
    needs: &'static str,
    /// Whether that dependency is satisfied.
    met: fn(&Config) -> bool,
    /// What the build produces instead.
    effect: &'static str,
    /// How to make it take effect, or how to stop asking.
    help: &'static str,
}

const INERT: &[Inert] = &[
    // A `bundle { }` block naming neither a collection nor the site binds no
    // pages, so it wrote no document and said nothing about it.
    Inert {
        setting: "generate { pdf { bundle } }",
        asked: |config| config.generate.pdf.bundle.present,
        needs: "a `collections` list or `site`",
        met: |config| config.generate.pdf.bundle.enabled(),
        effect: "no bundled document is written",
        help: "name the collections to bind (`collections \"guide\"`), or set `site #true` for the whole site",
    },
    // `minify` is documented as covering CSS and JS, but the JS handler is
    // gated on `bundle` in full: an unbundled `.js` is copied verbatim.
    Inert {
        setting: "assets { minify }",
        asked: |config| config.assets.minify,
        needs: "assets { bundle }",
        met: |config| config.assets.bundle,
        effect: "JavaScript is copied verbatim, and only stylesheets are minified",
        help: "turn on `assets { bundle }` to minify JavaScript too",
    },
    // Term feeds sit beside term listing pages, and a taxonomy publishes none
    // unless it asks: `terms` alone wrote no files and warned about nothing.
    Inert {
        setting: "generate { feed { terms } }",
        asked: |config| config.generate.feed.terms,
        needs: "a taxonomy with `listing`",
        met: |config| config.content.taxonomies.iter().any(|(_, t)| t.listing),
        effect: "no per-term feed is written",
        help: "set `listing` on the taxonomy whose terms should carry a feed",
    },
    // Both tune the prebuilt inverted index and reach no other format, so a
    // site on `formats \"json\"` tuned nothing at all.
    Inert {
        setting: "generate { search { stopwords } }",
        asked: |config| !config.generate.search.stopwords.is_empty(),
        needs: "generate { search { formats \"inverted\" } }",
        met: |config| config.generate.search.inverted(),
        effect: "the flat `json` index carries every token",
        help: "add `inverted` to `formats`, or drop the stopwords",
    },
    Inert {
        setting: "generate { search { minimum } }",
        asked: |config| config.generate.search.min_length != SearchConfig::default().min_length,
        needs: "generate { search { formats \"inverted\" } }",
        met: |config| config.generate.search.inverted(),
        effect: "the flat `json` index carries every token",
        help: "add `inverted` to `formats`, or drop the minimum",
    },
    // The verification artifacts are the point of pinning a `did`: without one
    // there is nothing to reference, and `verify` defaults on, so a site could
    // ask for both and get neither in silence.
    Inert {
        setting: "announce { standard { verify } }",
        asked: |config| {
            config
                .announce
                .standard
                .as_ref()
                .is_some_and(|s| s.verify.wellknown || s.verify.links)
        },
        needs: "announce { standard { did } }",
        met: |config| {
            config
                .announce
                .standard
                .as_ref()
                .is_some_and(|s| s.did.is_some())
        },
        effect: "no `.well-known` record and no per-page backlink are emitted, so the publication cannot be verified",
        help: "pin the account's `did`, or turn `verify` off",
    },
    // An `integrity` pins a digest to a URL. Where the URL is not
    // content-addressed, the file behind it changes while the pages naming it
    // stay cached, and every one of them then blocks the very stylesheet it
    // asked for. Stamping nothing is the safe half of that bargain.
    // The policy is written into `_headers` and nowhere else: it is a header,
    // and a static build has no other way to send one. Without that file the
    // whole block is a paragraph of config that produces nothing.
    Inert {
        setting: "security { csp }",
        asked: |config| config.security.csp.enabled,
        needs: "generate { headers }",
        met: |config| config.generate.headers,
        effect: "no policy is written, since `_headers` is the file it goes in",
        help: "turn on `generate { headers }`, or drop the `csp { }` block",
    },
    Inert {
        setting: "security { sri }",
        asked: |config| config.security.sri,
        needs: "assets { fingerprint }",
        met: |config| config.assets.fingerprint,
        effect: "no `integrity` attribute is stamped, since a digest pinned to a name that can change under it blocks the file it was meant to protect",
        help: "turn on `assets { fingerprint }`, which is what makes an asset URL name one exact file",
    },
];

impl Inert {
    /// Walk the table once against a site's config: name every setting that
    /// asked for something the config it sits in cannot deliver.
    fn resolve(config: &Config) -> Vec<SettingInert> {
        INERT
            .iter()
            .filter(|inert| (inert.asked)(config) && !(inert.met)(config))
            .map(SettingInert::from)
            .collect()
    }
}

impl From<&Inert> for SettingInert {
    fn from(inert: &Inert) -> Self {
        Self {
            setting: inert.setting,
            needs: inert.needs,
            effect: inert.effect,
            help: inert.help,
        }
    }
}

impl Gate {
    /// Walk the table once against a site's config: name every capability it
    /// asked for that this binary lacks, and turn `assets { fingerprint }` off
    /// when what would have kept it honest is missing.
    ///
    /// Turning it off rather than refusing the build is the same bargain every
    /// other gate strikes: the capability goes, the site still stands. It is
    /// applied here, before anything reads the config, so the whole build (the
    /// pipeline's renames, the render pass's rewrites, the cache fingerprint)
    /// agrees on one answer rather than disagreeing file by file.
    fn resolve(mut config: Config) -> (Config, Vec<FeatureMissing>) {
        let missing: Vec<&Self> = GATES
            .iter()
            .filter(|gate| !gate.compiled && (gate.asked)(&config))
            .collect();
        if missing.iter().any(|gate| gate.rewrites) {
            config.assets.fingerprint = false;
        }
        (
            config,
            missing.into_iter().map(FeatureMissing::from).collect(),
        )
    }
}

impl From<&Gate> for FeatureMissing {
    fn from(gate: &Gate) -> Self {
        Self {
            setting: gate.setting,
            cargo: gate.cargo,
            effect: gate.effect,
        }
    }
}

/// The build engine. Owns shared project state and drives the pipeline.
pub struct Engine {
    project: Project,
    config: Config,
    /// The resolved theme, when the site names one. Resolved once here rather
    /// than per consumer, since obtaining a package can download it.
    theme: Option<Theme>,
    /// What this binary cannot do that the site asked for, from [`Gate`].
    gaps: Vec<FeatureMissing>,
    /// What the site asked for that its own config withholds, from [`Inert`].
    inert: Vec<SettingInert>,
}

impl Engine {
    pub fn new(config: Config, mode: Mode) -> Result<Self> {
        // Ahead of everything, including the gates: this is the one check whose
        // failure destroys the project rather than the build. Checked here and
        // not at parse because `dist` is not settled until the profile overlay
        // and `--out` have had their say, and every command that writes to
        // `dist` comes through this constructor.
        if let Some((key, source)) = config.paths.swallowed(&config.root) {
            return Err(ConfigError::dist_contains_source(
                &config.paths.dist,
                key,
                &source.display().to_string(),
            )
            .into());
        }
        // A gate can turn a setting off, and a half-applied config is what ships
        // the broken site that guards against.
        let (config, gaps) = Gate::resolve(config);
        let inert = Inert::resolve(&config);
        let theme = Theme::of(&config)?;
        let project = Project::new(&config, mode)?;
        Ok(Self {
            project,
            config,
            theme,
            gaps,
            inert,
        })
    }

    /// Build the site incrementally: reuse cached output for unchanged pages,
    /// recompile the rest in parallel, then copy assets.
    ///
    /// Failure leaves `dist` as the previous build left it, staged assets
    /// included: `deploy` walks the whole directory, so a leftover staging tree
    /// would be uploaded as a duplicate copy of the site's assets.
    pub fn build(&self, ui: &Ui) -> Result<Stats> {
        let built = self.run(ui);
        if built.is_err() {
            let _ = std::fs::remove_dir_all(self.config.asset_staging());
        }
        built
    }

    /// A build, phase by phase. Each phase is a method below; this is the only
    /// place their order is spelled, and every constraint on that order is
    /// documented where it binds.
    fn run(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        let statics = self.stage()?;
        let planned = self.planned("planned build")?;
        let warned = ui.warnings();
        // What this binary cannot do that the site asked for. Reported per build
        // rather than per process: the dev server rebuilds in place, and the
        // warning belongs with the output it explains. `check` stays silent,
        // producing none of what a missing feature would have shaped.
        for gap in &self.gaps {
            ui.warn(*gap);
        }
        // ...and what its own config withholds from it. Same placement, same
        // reason: the setting that will not take effect belongs beside the
        // output that does not show it.
        for inert in &self.inert {
            ui.warn(*inert);
        }
        // `before` hooks run after the plan (a hook's output is this build's
        // assets, not this build's content) and ahead of the asset pipeline, so
        // anything they emit into `assets/` (e.g. Tailwind output) is
        // fingerprinted like any asset.
        let hooks = Hooks::new(&self.config);
        hooks.before(ui)?;
        // Built before the asset pipeline, whose `baudelaire:*` JS modules serve
        // the section trees it holds, and after it in [`Pass`], which renders
        // against what the pipeline produced.
        let prepare = self.prepare(&planned.pages)?;
        #[cfg(feature = "js")]
        let modules = Modules::new(self, &prepare);
        // the asset URL map feeds render-side fingerprint rewriting and folds
        // into the cache fingerprint, so a re-fingerprinted asset invalidates
        // the pages that reference it.
        let assets = Assets::new(
            &self.config,
            self.theme.as_ref(),
            #[cfg(feature = "js")]
            modules.ctx(&planned.pages),
        );
        let processed = assets.process()?;
        let (asset_count, asset_bytes) = (processed.count, processed.bytes);
        debug!(count = asset_count, bytes = asset_bytes, "assets processed");
        let mut emitted = processed.emitted;
        let pass = Pass::new(
            self,
            &planned,
            prepare,
            processed.map,
            processed.srcsets,
            // The renderer's own copy: a page is stamped with the digest of the
            // file the pipeline wrote, and the externalized images folded in
            // below carry none.
            emitted.clone(),
        );
        let mut cache = self.cache(&pass, &planned, ui)?;
        let (rendered, cached) = self.incremental(&pass, &mut cache, ui)?;
        // Ahead of validation, which weighs each page against what this build
        // actually wrote: a typst-embedded image is the usual way a picture
        // reaches a page, and a budget blind to those would count almost
        // nothing. Nothing here reads the pages, so the order is free.
        let images = self.images(&rendered, &cached, ui)?;
        emitted.absorb(images.emitted());
        self.validate(&rendered, &cached, Some(&emitted), false, ui)?;
        let outputs = Self::outputs(&rendered, &cached);
        Self::write(&outputs)?;
        // Bound from the page *sources*, so a bundle is exported whether its
        // pages recompiled or were served from cache.
        let bundled = self.bundles(&pass, &mut cache, ui)?;
        let artifacts: Vec<&Artifact> = rendered
            .iter()
            .flat_map(|r| &r.artifacts)
            .chain(bundled.drawn.iter())
            .collect();
        Self::artifacts(&artifacts)?;
        // Every page is on disk pointing at the new asset filenames, so the
        // staged asset tree can replace the published one. Before this line a
        // failure leaves `dist` exactly as the previous build left it.
        assets.publish()?;
        cache.save(outputs.iter().map(|out| (out.page, out.html)))?;
        let generated = self.generate(&planned.pages, &outputs, &statics, ui)?;
        self.sweep(&outputs, &statics, &generated, &bundled)?;
        // `after` hooks run once the whole site is on disk (deploy, Pagefind..).
        hooks.after(ui)?;

        // Warnings render as a block ahead of the result line, cargo-style.
        ui.flush();
        let total = rendered.len() + cached.len();
        let page_bytes: u64 = outputs.iter().map(|out| out.html.len() as u64).sum();
        let sidecars = Tally::of(artifacts.iter().copied());
        Summary {
            pages: total,
            cached: cached.len(),
            assets: asset_count + images.count(),
            statics: statics.count,
            generated: generated.count,
            bytes: page_bytes
                + asset_bytes
                + images.bytes()
                + generated.bytes
                + statics.bytes
                + sidecars.bytes,
            sidecars: sidecars.kinds,
            warnings: ui.warnings() - warned,
            dist: &self.config.paths.dist,
            elapsed: timer.elapsed(),
        }
        .report(ui);
        Ok(Stats {
            pages: total,
            cached: cached.len(),
        })
    }

    /// Prepare `dist` and seed it with the static tree.
    ///
    /// A staging tree here is a previous build's failure; it is cleared before
    /// the static copy, which writes into it (see `Static::destination`). Static
    /// goes down first so a generated page or asset at the same output path
    /// overwrites it: static is the lowest-priority source.
    fn stage(&self) -> Result<Copied> {
        fs::create_dir_all(&self.config.paths.dist)?;
        let _ = std::fs::remove_dir_all(self.config.asset_staging());
        let statics = Static::new(&self.config, self.theme.as_ref()).copy()?;
        debug!(
            count = statics.count,
            bytes = statics.bytes,
            "static copied"
        );
        Ok(statics)
    }

    /// Plan the pages this pass covers, alongside the tracked value trees every
    /// consumer of them borrows. `what` names the pass in the trace line.
    fn planned(&self, what: &'static str) -> Result<Planned> {
        let pages = plan(&self.config, &self.project)?;
        debug!(pages = pages.len(), site = self.config.label(), "{what}");
        Ok(Planned {
            pages,
            tracked: self.project.tracked(),
        })
    }

    /// The compile inputs for `pages`: the wrapper text binding each page to its
    /// template, plus the section trees derived from the whole page set.
    ///
    /// Built by the caller rather than by [`Pass`], because a build feeds these
    /// section trees to the JS bundler before the asset pipeline runs, and the
    /// asset pipeline is what a [`Pass`] renders against.
    ///
    /// Writing the tree out is part of building the inputs, not a step a caller
    /// can forget: a template's `#import` of it has to resolve on the very
    /// first compile, including in a fresh checkout where nothing has run yet.
    fn prepare<'a>(&'a self, pages: &'a [Page]) -> Result<Prepare<'a>> {
        let prepare = Prepare::new(&self.config, &self.project, self.theme.as_ref(), pages);
        for file in prepare.generated() {
            file.write(self.project.root())?;
        }
        // A content page that imports a table was served the empty one during
        // discovery (the table is derived from the frontmatter that read was
        // producing). Now that the real ones are on disk, the world drops what
        // it cached, so the compile reads what this build wrote.
        self.project.tables_written();
        Ok(prepare)
    }

    /// Load the build cache, keyed on every site-wide input the per-page
    /// dependency tracker cannot see.
    fn cache(&self, pass: &Pass, planned: &Planned, ui: &Ui) -> Result<Cache> {
        // render-side cache inputs: asset renames, the link map, the responsive
        let render = RenderInputs {
            modules: self.project.modules(),
        };
        Cache::load(
            &self.config,
            &render,
            planned.tracked.clone(),
            pass.renderer.maps(),
            self.project.root(),
            ui,
        )
    }

    /// Serve what the cache still covers, compile the rest, and record every
    /// fresh page against the manifest the next build reads.
    fn incremental<'a>(
        &self,
        pass: &Pass<'a>,
        cache: &mut Cache,
        ui: &Ui,
    ) -> Result<(Vec<Rendered<'a>>, Vec<Reused<'a>>)> {
        let (cached, stale) = pass.split(cache);
        debug!(stale = stale.len(), reused = cached.len(), "cache split");
        // Compile only the stale pages (already prepared during the cache
        // split); cached pages keep the HTML they were built with.
        let rendered = self.render_pages("compiling", stale, ui, |(page, prepared)| {
            (
                page,
                prepared.and_then(|(id, text, fp)| self.compile(page, id, text, fp, pass)),
            )
        })?;
        for r in &rendered {
            cache.record(r.into());
        }
        for (page, _, _) in &cached {
            ui.page(self.relative(page), PageStatus::Cached);
        }
        Ok((rendered, cached))
    }

    /// Copy every page's externalized images into the (freshly regenerated)
    /// asset directory: fresh pages carry their refs, cache hits their stored
    /// ones, so the files are present regardless of what recompiled.
    fn images(&self, rendered: &[Rendered], cached: &[Reused], ui: &Ui) -> Result<Images> {
        Images::new(&self.config, self.project.root()).copy(
            rendered
                .iter()
                .flat_map(|r| &r.outputs.images)
                .chain(cached.iter().flat_map(|(_, _, out)| &out.images)),
            ui,
        )
    }

    /// Pair every page, rendered and cache-served alike, with what the render
    /// pass produced for it: the write pass, the blob staging, and the
    /// processors all read this one view.
    fn outputs<'a>(rendered: &'a [Rendered<'a>], cached: &'a [Reused<'a>]) -> Vec<Output<'a>> {
        rendered
            .iter()
            .map(|r| Output {
                page: r.page,
                html: r.html.as_str(),
                fragments: r.outputs.fragments.as_ref(),
                inline: &r.outputs.inline,
            })
            .chain(cached.iter().map(|(page, html, out)| Output {
                page,
                html: html.as_str(),
                fragments: out.fragments.as_ref(),
                inline: &out.inline,
            }))
            .collect()
    }

    /// Write every page's HTML in parallel: independent files, no shared state.
    fn write(outputs: &[Output]) -> Result<()> {
        outputs
            .par_iter()
            .try_for_each(|out| fs::write_all(&out.page.output, out.html))
    }

    /// Write every artifact this build produced beside the pages: the sidecars
    /// drawn during compile, and the bundled documents.
    ///
    /// Only what was freshly made is here. A cache hit leaves the file the
    /// previous build wrote in place, and the sweep keeps it. Each artifact
    /// carries its own destination, so nothing here has to know which kind it
    /// is holding.
    fn artifacts(artifacts: &[&Artifact]) -> Result<()> {
        artifacts
            .par_iter()
            .try_for_each(|artifact| fs::write_all(&artifact.path, &artifact.bytes))
    }

    /// Export the bundled documents this site asks for: a collection, or the
    /// whole site, as one PDF.
    ///
    /// Unlike a sidecar this belongs to no page, so it cannot ride a page's
    /// cache entry: it carries one of its own, keyed on the module text (which
    /// names every page it binds, in order) and on every file that compile
    /// read.
    #[cfg(feature = "pdf")]
    fn bundles(&self, pass: &Pass<'_>, cache: &mut Cache, ui: &Ui) -> Result<Bundled> {
        let mut bundled = Bundled::default();
        for bundle in Bundle::planned(&self.config, pass.pages) {
            let path = bundle.path(&self.config);
            bundled.paths.push(path.clone());
            let text = bundle.source(&pass.prepare, &self.project)?;
            let fingerprint = Hash::of_bytes(text.as_bytes());
            if cache.reuse_bundle(bundle.id(), &fingerprint, &path) {
                debug!(bundle = bundle.id(), "bundle reused");
                continue;
            }
            let (bytes, deps) = bundle.export(&self.project, &pass.prepare, text)?;
            cache.record_bundle(bundle.id(), fingerprint, &deps);
            ui.page(bundle.id(), PageStatus::Built);
            bundled.drawn.push(Artifact {
                kind: Bundle::KIND,
                path,
                bytes,
            });
        }
        Ok(bundled)
    }

    /// Without the exporter nothing binds one, so there is nothing to write and
    /// nothing to keep.
    ///
    /// It mirrors the `pdf`-on signature exactly so the caller compiles
    /// unchanged in both flavors, which is why it takes a `self` it cannot use
    /// and returns a `Result` it cannot fail.
    #[cfg(not(feature = "pdf"))]
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn bundles(&self, _pass: &Pass<'_>, _cache: &mut Cache, _ui: &Ui) -> Result<Bundled> {
        Ok(Bundled::default())
    }

    /// Run the post-build processors over the finished site.
    fn generate(
        &self,
        pages: &[Page],
        outputs: &[Output],
        statics: &Copied,
        ui: &Ui,
    ) -> Result<Generated> {
        let site = Site {
            config: &self.config,
            pages,
            outputs,
        };
        let mut emitter = Emitter::new(ui, statics.paths.iter().cloned());
        Processors::builtin().run(&site, &mut emitter)?;
        Ok(Generated {
            count: emitter.written(),
            bytes: emitter.bytes(),
            paths: emitter.paths().to_vec(),
        })
    }

    /// Drop orphaned outputs from earlier builds (a removed page or taxonomy
    /// term, a renamed permalink) so `dist` never serves stale files.
    ///
    /// Gated on `prune` so a user managing `dist` by hand can opt out. The
    /// keep-set is every file this build produced: page HTML, static
    /// passthrough, generated files. The asset tree is regenerated wholesale, so
    /// the prune skips it. Runs before `after` hooks, whose outputs (Pagefind..)
    /// are not ours to prune.
    fn sweep(
        &self,
        outputs: &[Output],
        statics: &Copied,
        generated: &Generated,
        bundled: &Bundled,
    ) -> Result<()> {
        if !self.config.prune {
            return Ok(());
        }
        // A sidecar belongs to its page whether or not this build re-drew it,
        // so the keep set is derived from the pages, never from what was written.
        let sidecars = Sidecars::builtin();
        let drawn = outputs
            .iter()
            .flat_map(|out| sidecars.planned(&self.config, out.page));
        let keep: Vec<PathBuf> = outputs
            .iter()
            .map(|out| out.page.output.clone())
            .chain(drawn)
            .chain(bundled.paths.iter().cloned())
            .chain(statics.paths.iter().cloned())
            .chain(generated.paths.iter().cloned())
            .collect();
        let pruned = prune::Prune::new(
            &self.config.paths.dist,
            &self.config.asset_dist(),
            &self.config.cache.dir,
        )
        .run(&keep)?;
        debug!(pruned, "orphaned outputs removed");
        Ok(())
    }

    /// Compile every page and report diagnostics without writing any output.
    pub fn check(&self, ui: &Ui) -> Result<Stats> {
        let timer = Timer::start();
        let planned = self.planned("planned check")?;
        // Nothing is written, so nothing was fingerprinted: the render pass has
        // no asset renames to apply and no variants to offer.
        let pass = Pass::new(
            self,
            &planned,
            self.prepare(&planned.pages)?,
            AssetMap::new(self.config.asset_prefix()),
            SrcSets::default(),
            Emitted::default(),
        );
        // Prepare + compile every page inline (no cache split, no output).
        let rendered = self.render_pages("checking", pass.pages.iter().collect(), ui, |page| {
            (
                page,
                pass.prepare
                    .input(page)
                    .and_then(|(id, text, fp)| self.compile(page, id, text, fp, &pass)),
            )
        })?;
        // No asset pipeline ran and nothing was written, so there is nothing to
        // weigh a page's loads against: `check` lints, and leaves the budgets to
        // the build that produces the bytes.
        self.validate(&rendered, &[], None, true, ui)?;
        ui.flush();
        ui.done(format_args!(
            "checked {} in {}",
            Count::pages(rendered.len()),
            Dur(timer.elapsed())
        ));
        Ok(Stats {
            pages: rendered.len(),
            cached: 0,
        })
    }

    /// Compile a batch of pages in parallel and reduce to their rendered
    /// outputs: a progress bar, a rayon map producing one `(page, outcome)` per
    /// item, then [`Engine::collect`] (status lines + diagnostics). The shared
    /// spine of `build` (over the stale subset) and `check` (every page);
    /// `outcome` supplies the only difference: how one item renders. Validation
    /// is the caller's, since only `build` has cached pages to replay.
    fn render_pages<'a, T: Send>(
        &self,
        label: &'static str,
        items: Vec<T>,
        ui: &Ui,
        outcome: impl Fn(T) -> (&'a Page, Result<Rendered<'a>>) + Sync,
    ) -> Result<Vec<Rendered<'a>>> {
        let progress = ui.progress(label, items.len());
        let outcomes: Vec<(&Page, Result<Rendered>)> = items
            .into_par_iter()
            .map(|item| {
                let (page, out) = outcome(item);
                progress.tick(self.relative(page));
                (page, out)
            })
            .collect();
        progress.finish();
        self.collect(outcomes, ui)
    }

    /// Report each compile outcome (page status lines and any typst warnings
    /// the compiler raised), returning the rendered pages or, after every
    /// failure has been reported, an error carrying *all* failed pages'
    /// diagnostics (a single failure propagates unchanged).
    fn collect<'a>(
        &self,
        outcomes: Vec<(&'a Page, Result<Rendered<'a>>)>,
        ui: &Ui,
    ) -> Result<Vec<Rendered<'a>>> {
        let mut errors = Vec::new();
        let mut rendered = Vec::new();
        for (page, outcome) in outcomes {
            match outcome {
                Ok(mut r) => {
                    ui.page(self.relative(page), PageStatus::Built);
                    for warning in r.warnings.drain(..) {
                        ui.warn(warning);
                    }
                    rendered.push(r);
                }
                Err(e) => {
                    ui.page(self.relative(page), PageStatus::Failed);
                    errors.push(e);
                }
            }
        }
        match BuildFailed::aggregate(errors) {
            Some(e) => Err(e),
            None => Ok(rendered),
        }
    }

    /// A page's source path relative to the content root, for display. Handles
    /// both discovered pages (content-relative) and generated pages (whose
    /// synthetic sources are canonical-absolute).
    fn relative(&self, page: &Page) -> String {
        let canonical = fs::canonical(&self.config.paths.content);
        page.source
            .strip_prefix(canonical)
            .or_else(|_| page.source.strip_prefix(&self.config.paths.content))
            .unwrap_or(&page.source)
            .display()
            .to_string()
    }

    /// Compile a single page to rendered HTML, applying render post-processing
    /// (link rewriting over the typed DOM) before serialization. Records the
    /// files typst read so the cache can invalidate the page precisely, and
    /// keeps typst's own compile warnings so the build can surface them.
    fn compile<'a>(
        &self,
        page: &'a Page,
        id: FileId,
        text: String,
        fingerprint: Hash,
        pass: &Pass<'_>,
    ) -> Result<Rendered<'a>> {
        // parse only now, for a page actually being (re)compiled.
        let source = Source::new(id, text);
        let world = Tracked::new(self.project.world_for(&source));
        let compiled = typst::compile::<HtmlDocument>(&world);
        // typst warnings (unknown font families, deprecations..) survive a
        // successful compile; bridge them like errors so they render with
        // spans. On failure they are dropped: the errors say more. Typst's
        // blanket "html export is under active development" notice is filtered:
        // HTML is this tool's entire output, so it would fire on every page of
        // every build while telling the author nothing actionable.
        let warnings = compiled
            .warnings
            .into_iter()
            .filter(|w| {
                !w.message
                    .starts_with("html export is under active development")
            })
            .collect();
        let warnings = Self::diagnostics(warnings, page, &source, world.inner());
        let mut doc = compiled.output.map_err(|errs| {
            BaudelaireErrorKind::TypstCompile(Self::diagnostics(errs, page, &source, world.inner()))
        })?;
        let mut rewrite = pass
            .renderer
            .rewrite(&mut doc, page, &self.config, world.inner());
        // An icon that could not be inlined leaves an empty `<svg>` in the DOM,
        // so the page cannot be shipped: fail on the first one.
        // Only the first is reported: the files are independent, and stopping
        // at one error is the contract every other pass has.
        if let Some(invalid) = std::mem::take(&mut rewrite.invalid).into_iter().next() {
            return Err(invalid.into());
        }
        let options = HtmlOptions {
            pretty: self.config.pretty(),
        };
        // Shared by both serializations below, so a failure in either reports
        // with the page's own spans.
        let serialization_failed = |errs| {
            BaudelaireErrorKind::TypstHtml(Self::diagnostics(errs, page, &source, world.inner()))
        };
        let html = typst_html::html(&doc, &options).map_err(&serialization_failed)?;
        // Only the single-file export consumes these, and capturing them costs
        // a second pass over the DOM, so nothing else pays for it.
        let fragments = self
            .config
            .navigation
            .standalone
            .enabled
            .then(|| Fragments::capture(&doc, &options).map_err(&serialization_failed))
            .transpose()?;
        // A sidecar is a second, paged compile of the same page, so only a stale
        // page pays for one. A cache hit keeps the file already in `dist`.
        let (artifacts, drawn) =
            pass.sidecars
                .draw(&self.project, &self.config, &pass.prepare, page)?;
        let mut deps = self.project.dependencies(&world);
        // Inlined icons and embedded assets are read by the render pass, not by
        // typst, so they are absent from the compilation's own accesses. Adding
        // them here puts them under the same content-hash check as every other
        // dependency, instead of needing a mechanism of their own.
        deps.extend(std::mem::take(&mut rewrite.read));
        // A sidecar is a second compile of this page, so the template it imports
        // and everything that template pulls in are inputs to this page's output
        // that its own compile never read. Folded in the same way, and for the
        // same reason: without them an edited card helper changed no hash the
        // cache checks, and every card-bearing page stayed a hit still serving
        // the PNG the old helper drew.
        deps.extend(drawn.files().iter().cloned());
        // Which injected values (`sys.inputs.baudelaire.*`) the page read, across
        // its own source and every `.typ` it depends on: the fine-grained
        // metadata dependency set.
        let mut reads = pass.analyzer.reads(&source, &deps);
        // `datetime.today()` goes through the `World`, which records files
        // only, so nothing else sees it: a page printing the current year stayed
        // a cache hit into the next one. It reads the same clock as
        // `sys.inputs.baudelaire.date`, so record it as that key.
        if world.reads_clock() {
            reads.insert(Project::clock());
        }
        Ok(Rendered {
            page,
            fingerprint,
            html,
            deps,
            reads,
            links: rewrite.links,
            srcsets: rewrite.srcsets,
            assets: rewrite.assets,
            outputs: Outputs {
                images: rewrite.images,
                broken: rewrite.broken,
                anchors: rewrite.anchors,
                deep: rewrite.deep,
                fragments,
                lints: rewrite.lints,
                weight: rewrite.weight,
                inline: rewrite.inline,
            },
            external: rewrite.external,
            artifacts,
            warnings,
        })
    }

    /// Run the post-render validation passes over every page, compiled and
    /// cached alike. Maps each into the decoupled [`Compiled`] view and hands it
    /// to [`Links`]; the pass itself lives in [`check`].
    ///
    /// A cache hit replays the broken links it was built with, because checking
    /// only fresh pages made the gate weaken on rebuild: a second build of a
    /// site with a dangling link reported nothing and `links { strict #true }`
    /// passed.
    ///
    /// `outbound` reaches the network and so is passed only by [`Engine::check`],
    /// which recompiles every page and therefore sees every outbound link. A
    /// build stays offline whatever the config says.
    fn validate(
        &self,
        rendered: &[Rendered],
        cached: &[(&Page, String, Outputs)],
        emitted: Option<&Emitted>,
        outbound: bool,
        ui: &Ui,
    ) -> Result<()> {
        // Cached pages contribute no outbound links: nothing recompiled them, so
        // nothing re-read their anchors. Only `check` asks for them, and it never
        // serves a page from cache.
        let fresh = rendered
            .iter()
            .map(|r| (r.page, r.html.as_str(), &r.outputs, r.external.as_slice()));
        let reused = cached
            .iter()
            .map(|(page, html, outputs)| (*page, html.as_str(), outputs, &[] as &[String]));
        let pages: Vec<CheckedPage> = fresh
            .chain(reused)
            .map(|(page, html, outputs, external)| CheckedPage {
                label: self.relative(page),
                source: &page.source,
                permalink: &page.permalink,
                broken: &outputs.broken,
                external,
                anchors: &outputs.anchors,
                deep: &outputs.deep,
                lints: &outputs.lints,
                weight: &outputs.weight,
                html,
            })
            .collect();
        let site = Compiled {
            config: &self.config,
            pages: &pages,
            emitted,
        };
        Links::run(&site, ui)?;
        if self.config.lint.enabled {
            Lints::run(&site, ui)?;
            Budgets::run(&site)?;
        }
        if outbound && self.config.links.external {
            External::run(&site, ui)?;
        }
        Ok(())
    }

    /// Wrap typst source diagnostics with the compiled source so miette renders
    /// spans against exactly what was compiled (the layout-wrapped source, when
    /// a template is bound). Shared by the compile and HTML-emit stages.
    fn diagnostics(
        errs: typst::ecow::EcoVec<typst::diag::SourceDiagnostic>,
        page: &Page,
        source: &Source,
        world: &PageWorld,
    ) -> Vec<TypstSourceDiagnostic> {
        TypstSourceDiagnostic::bridge(
            errs,
            (&page.source.display().to_string(), source.text()),
            Arc::new(world.clone()),
        )
    }
}

/// The owned inputs one pass over the site borrows: the planned pages and the
/// build's tracked value trees. Separate from [`Pass`] because [`Pass`] holds
/// borrows of both, and no struct can borrow from itself.
struct Planned {
    pages: Vec<Page>,
    /// The injected values whose per-page reads drive fine-grained metadata
    /// invalidation: the analyzer records them from each page's syntax, the
    /// cache re-hashes them to decide reuse. One owned copy backs the cache; the
    /// analyzer borrows another view of the same trees.
    tracked: Vec<(String, Value)>,
}

/// A page served from the cache: the HTML the build that compiled it produced,
/// and the render-pass outputs recorded alongside.
type Reused<'a> = (&'a Page, String, Outputs);

/// Everything a compile pass over the site shares: the pages it covers, their
/// compile inputs, the render layer they are rewritten through, and the analyzer
/// that records which injected values each one read.
///
/// Built once and consumed by both [`Engine::run`] and [`Engine::check`], which
/// otherwise derived the same four values side by side and had to be kept in
/// step by hand.
struct Pass<'a> {
    config: &'a Config,
    pages: &'a [Page],
    prepare: Prepare<'a>,
    renderer: Renderer,
    analyzer: Analyzer<'a>,
    /// The artifacts drawn beside each page's HTML, registered once for the
    /// pass rather than rebuilt per page inside the pool.
    sidecars: Sidecars,
}

impl<'a> Pass<'a> {
    /// Wire a pass over `planned`, rendering against `assets`, `srcsets` and
    /// `emitted`: what the asset pipeline produced for a build, empty for a
    /// check, which rewrites nothing it will not write.
    fn new(
        engine: &'a Engine,
        planned: &'a Planned,
        prepare: Prepare<'a>,
        assets: AssetMap,
        srcsets: SrcSets,
        emitted: Emitted,
    ) -> Self {
        Self {
            config: &engine.config,
            pages: &planned.pages,
            prepare,
            renderer: Renderer::new(
                &planned.pages,
                assets,
                srcsets,
                emitted,
                engine.project.root(),
            ),
            analyzer: Analyzer::new(
                planned.tracked.iter().map(Root::from).collect::<Roots>(),
                &engine.project,
            ),
            sidecars: Sidecars::builtin(),
        }
    }

    /// Split the pages into cache hits and stale ones.
    ///
    /// [`Prepare`] produces each page's text and fingerprint without parsing it;
    /// the parse into a typst `Source` is deferred to the compile, so a hit
    /// never pays to parse a page it won't render. A page whose input could not
    /// be built is stale, so its error is reported by the compile pass with
    /// every other page's.
    fn split(&self, cache: &mut Cache) -> (Vec<Reused<'a>>, Vec<(&'a Page, Result<Prepared>)>) {
        let mut cached = Vec::new();
        let mut stale = Vec::new();
        for page in self.pages {
            match self.prepare.input(page) {
                Ok((id, text, fingerprint)) => {
                    // The existence check comes first so a page that has to be
                    // recompiled anyway leaves no reuse bookkeeping behind.
                    match self
                        .drawn(page)
                        .then(|| cache.reuse(page, &fingerprint))
                        .flatten()
                    {
                        Some((html, outputs)) => cached.push((page, html, outputs)),
                        None => stale.push((page, Ok((id, text, fingerprint)))),
                    }
                }
                Err(e) => stale.push((page, Err(e))),
            }
        }
        (cached, stale)
    }

    /// Whether every file this page's sidecars own is still on disk.
    ///
    /// A page's HTML is rewritten on every build, hit or miss, because the cache
    /// holds the markup itself. A sidecar is not: only the build that compiles a
    /// page draws one, so once the file is gone (a deleted `dist`, a hand-removed
    /// card, a half-finished copy) nothing would ever draw it again and the cache
    /// would keep reporting the page as built. Missing one makes the page stale,
    /// which is the only thing that redraws it.
    fn drawn(&self, page: &Page) -> bool {
        self.sidecars
            .planned(self.config, page)
            .iter()
            .all(|path| path.exists())
    }
}

/// The site values the `baudelaire:*` virtual JS modules serve, built from the
/// same wrapper inputs the templates get rather than recomputed from scratch.
///
/// Owned by the build, because [`JsCtx`] borrows them for as long as the asset
/// pipeline lives.
#[cfg(feature = "js")]
struct Modules {
    /// The codegen `Value` view of the build context. It exists only for these
    /// modules; Typst reads `sys.inputs` from the raw context.
    context: Value,
    /// The section trees keyed by language: one bundle serves the whole site, so
    /// a default-language-only tree left every translation without a nav.
    /// Templates get their own wrapper text and never read this.
    sections: Value,
}

#[cfg(feature = "js")]
impl Modules {
    fn new(engine: &Engine, prepare: &Prepare) -> Self {
        Self {
            sections: Value::dict(
                engine
                    .config
                    .langs()
                    .into_iter()
                    .map(|lang| (lang.to_owned(), prepare.sections(lang))),
            ),
            context: Value::from(engine.project.context()),
        }
    }

    /// The bundler's view of this build, completed with the page set.
    fn ctx<'a>(&'a self, pages: &'a [Page]) -> JsCtx<'a> {
        JsCtx {
            pages,
            context: &self.context,
            sections: &self.sections,
        }
    }
}

/// A compiled page ready to write, with the files its compilation depended on,
/// the raw targets of any broken internal links it contained, and the warnings
/// typst raised while compiling it.
struct Rendered<'a> {
    page: &'a Page,
    fingerprint: Hash,
    html: String,
    deps: Deps,
    reads: Reads,
    /// The permalinks this page's links resolved against: a render-side
    /// dependency the compile itself never sees, since typst does not read a
    /// link target's source.
    links: LinkDeps,
    /// The responsive variants this page's images matched: generated by the
    /// asset pipeline and matched render-side, so the compile never sees them.
    srcsets: SrcSetDeps,
    /// The asset-map entries this page's references resolved through.
    assets: AssetDeps,
    /// The render pass's own results (externalized images, broken links), the
    /// same shape the cache stores and replays for a hit.
    outputs: Outputs,
    /// Outbound `http(s)` links the page carries, for `check --external`. Not
    /// cached: only a fresh compile collects them, and only `check` reads them.
    external: Vec<String>,
    /// The files this page produces beside its HTML (a social card..), each
    /// with the destination it was drawn for.
    artifacts: Vec<Artifact>,
    warnings: Vec<TypstSourceDiagnostic>,
}

/// The cache stores the subset of a compile that survives it. The rest
/// (outbound links, the sidecar files, warnings) is consumed by this build alone.
impl<'a> From<&'a Rendered<'a>> for Recorded<'a> {
    fn from(rendered: &'a Rendered<'a>) -> Self {
        Self {
            page: rendered.page,
            fingerprint: rendered.fingerprint,
            html: &rendered.html,
            deps: &rendered.deps,
            reads: &rendered.reads,
            links: &rendered.links,
            srcsets: &rendered.srcsets,
            assets: &rendered.assets,
            outputs: &rendered.outputs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GATES, Gate, INERT, Inert};
    use crate::config::Config;

    fn config(text: &str) -> Config {
        Config::parse(text).expect("should parse")
    }

    /// Content-hashing renames a file that other files name. Without the `css`
    /// feature nothing rewrites the `url()` inside a stylesheet, so the sheet is
    /// served naming assets that no longer exist: a green build and a 404 site.
    /// Fingerprinting is turned off for the whole build instead, and said so.
    #[test]
    fn fingerprinting_without_the_stylesheet_rewriter_is_turned_off_and_reported() {
        let asked = config("assets { fingerprint #true }");
        assert!(asked.assets.fingerprint, "the site asked for it");
        let (resolved, gaps) = Gate::resolve(asked);
        assert_eq!(
            resolved.assets.fingerprint,
            cfg!(feature = "css"),
            "kept where stylesheets can be rewritten, dropped where they cannot"
        );
        assert_eq!(
            gaps.iter()
                .any(|gap| gap.setting == "assets { fingerprint }" && gap.cargo == "css"),
            !cfg!(feature = "css"),
            "turning a setting off is never silent"
        );
    }

    /// The walk only ever fires on a setting the site opted into, so a config
    /// that asks for nothing optional is untouched whatever this binary lacks.
    #[test]
    fn a_site_asking_for_nothing_optional_is_untouched() {
        let (resolved, gaps) = Gate::resolve(config(""));
        assert!(!resolved.assets.fingerprint);
        assert!(gaps.is_empty());
    }

    /// A gate names the config that asks for it, and codes read as identity, so
    /// two rows describing the same setting would report the same gap twice.
    #[test]
    fn every_gate_names_a_distinct_setting() {
        for (i, gate) in GATES.iter().enumerate() {
            assert!(
                !GATES[i + 1..].iter().any(|o| o.setting == gate.setting),
                "`{}` is claimed by two gates",
                gate.setting
            );
        }
    }

    /// Every row fires on the config it describes, and stops once the setting
    /// it depends on is there. Written as the pair, because a warning that
    /// cannot be silenced by doing what it asks is worse than none.
    #[test]
    fn each_inert_setting_reports_until_its_dependency_is_set() {
        let cases = [
            (
                "assets { minify }",
                "assets { minify #true }",
                "assets { minify #true; bundle #true }",
            ),
            (
                "generate { feed { terms } }",
                "generate { feed { formats \"rss\"; terms #true } }",
                "generate { feed { formats \"rss\"; terms #true } }\ncontent { taxonomies { tags listing=#true } }",
            ),
            (
                "generate { search { stopwords } }",
                "generate { search { formats \"json\"; stopwords \"the\" } }",
                "generate { search { formats \"inverted\"; stopwords \"the\" } }",
            ),
            (
                "generate { search { minimum } }",
                "generate { search { formats \"json\"; minimum 4 } }",
                "generate { search { formats \"inverted\"; minimum 4 } }",
            ),
            (
                "announce { standard { verify } }",
                "announce { standard { handle \"a.example\" } }",
                "announce { standard { handle \"a.example\"; did \"did:plc:x\" } }",
            ),
        ];
        for (setting, asked, satisfied) in cases {
            let named = |text| {
                Inert::resolve(&config(text))
                    .iter()
                    .any(|i| i.setting == setting)
            };
            assert!(named(asked), "`{setting}` did not report on `{asked}`");
            assert!(
                !named(satisfied),
                "`{setting}` still reports once its dependency is set"
            );
        }
    }

    /// The counterpart of [`every_gate_names_a_distinct_setting`].
    #[test]
    fn every_inert_row_names_a_distinct_setting() {
        for (i, inert) in INERT.iter().enumerate() {
            assert!(
                !INERT[i + 1..].iter().any(|o| o.setting == inert.setting),
                "`{}` is claimed by two rows",
                inert.setting
            );
        }
    }
}
