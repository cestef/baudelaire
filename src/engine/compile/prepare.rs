//! The compile input for a page: the synthetic Typst module that binds it to
//! its template, and the values injected into that module.
//!
//! Everything a page's wrapper text is made of lives here: which import root
//! the template comes from, the sibling and translation links, the UI strings.
//! They are wrapper *text*, so a change to any of them refingerprints the pages
//! that read it and the cache stays correct, which is why they are built once
//! for the whole site rather than per page.
//!
//! The section tree and the page catalogue are the deliberate exceptions: each
//! names every page on the site, so as wrapper text they would tie every page's
//! fingerprint to every other page's title and URL. They go to [`Generated`]
//! instead, and reach templates as files they import.
//!
//! [`Layout`] renders the text; this decides what goes into it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};

use crate::codegen::{Typst, Value};
use crate::config::Config;
use crate::content::{Data, Iso, Localized, Page, Section, Sibling, Siblings, Strings};
use crate::error::Result;
use crate::graph::Hash;
use crate::render::Backlinks;
use crate::theme::Theme;
use crate::world::Project;
use crate::world::module;

use super::layout::{Bind, Body, Context, Layout};
use crate::world::generated::Table;

/// A page reduced to what the cache check needs: its `FileId`, the exact text
/// typst will compile, and that text's fingerprint, before the costly parse
/// into a `Source` (deferred to the compile, run only for stale pages).
pub(in crate::engine) type Prepared = (FileId, String, Hash);

/// Builds every page's compile input against one build's shared state: the site
/// config, the project world, the resolved theme, and the section trees, which
/// are derived from the whole page set and so cost one pass rather than one per
/// page.
pub(in crate::engine) struct Prepare<'a> {
    config: &'a Config,
    project: &'a Project,
    theme: Option<&'a Theme>,
    pages: &'a [Page],
    /// One section tree per built language. Not wrapper text: the tree reaches
    /// templates as a file ([`Generated`]) and the JS bundler as a value, and
    /// putting it in the wrapper would make every page's fingerprint depend on
    /// every other page's title and URL.
    trees: BTreeMap<String, Value>,
    /// The template directory as the compiler spells it, resolved once: every
    /// page's wrapper names it, and canonicalizing per page would pay for the
    /// same answer once per page.
    templates: PathBuf,
    /// The backlinks each page is compiled against.
    ///
    /// A *prediction* until the site has rendered: it starts as what the last
    /// build recorded (or nothing, on a cold one), and the repair pass replaces
    /// it with the graph this build actually produced before recompiling the
    /// pages that disagree. [`Backlinks::Off`] until a build sets one, which is
    /// also what a site with the feature off compiles against.
    backlinks: Backlinks,
}

impl<'a> Prepare<'a> {
    pub(in crate::engine) fn new(
        config: &'a Config,
        project: &'a Project,
        theme: Option<&'a Theme>,
        pages: &'a [Page],
    ) -> Self {
        let base = Self {
            config,
            project,
            theme,
            pages,
            trees: BTreeMap::new(),
            templates: config.paths.under(project.root()).templates,
            backlinks: Backlinks::Off,
        };
        // Built once from the whole page set and shared by every consumer, so
        // the file templates import, the JS module, and any future reader can
        // never disagree about the site's shape.
        let trees = config
            .langs()
            .iter()
            .map(|lang| ((*lang).to_owned(), base.sections(lang)))
            .collect();
        Self { trees, ..base }
    }

    /// Compile the next pages against `backlinks`: what the last build recorded
    /// before pass one, what this build produced before a repair.
    pub(in crate::engine) fn assume(&mut self, backlinks: Backlinks) {
        self.backlinks = backlinks;
    }

    /// The digest of the backlinks `page` was compiled with, which the repair
    /// pass checks the finished site's graph against.
    ///
    /// `None` where the page never saw them: the feature is off, or the page has
    /// no template and so no wrapper to carry the value. Recording a digest for
    /// one of those would claim the compiled text holds something it does not,
    /// and recompile the page to byte-identical output whenever the graph moved.
    pub(in crate::engine) fn digest(&self, page: &Page) -> Option<Hash> {
        page.template.as_ref()?;
        self.backlinks.digest(page)
    }

    /// The site config this pass renders against. Read by the bundle, which
    /// resolves its own template and title from it.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn config(&self) -> &Config {
        self.config
    }

    /// The files templates import, ready to write: the section tree and the
    /// page catalogue.
    ///
    /// One per file-backed module, as an array sized by that registry, so a
    /// module added there without a table to write it fails to compile rather
    /// than resolving to a file nothing produces.
    pub(in crate::engine) fn generated(&self) -> [Table; module::FILES.len()] {
        [
            Table::new(module::SECTIONS, self.trees.clone()),
            Table::new(
                module::PAGES,
                Page::catalogue(self.pages, self.config)
                    .into_iter()
                    .map(|(lang, rows)| (lang, Value::array(rows)))
                    .collect(),
            ),
        ]
    }

    /// The compile input for a page: its (possibly synthetic) source and its
    /// content fingerprint: a hash of the exact text typst compiles. A real
    /// page's body reaches the compiler through `#include` (a tracked file
    /// read, so the dependency cache covers its edits); only generated
    /// listings, which have no file, inline their body, and only their
    /// wrapper text needs fingerprinting. Built once and shared by the cache
    /// check and the compile.
    pub(in crate::engine) fn input(&self, page: &Page) -> Result<Prepared> {
        let rooted = self.project.virtualize(&page.source)?;
        let Some(template) = &page.template else {
            let text = page.body.clone();
            let fingerprint = Hash::of_bytes(text.as_bytes());
            return Ok((FileId::new(rooted), text, fingerprint));
        };
        let id = match &page.data {
            Data::Generated(_) => FileId::new(rooted.clone()),
            _ => Self::wrapper(&rooted),
        };
        let dir = self.dir(template);
        // The page's identity is its wrapper as it would read with *no*
        // backlinks, and the text it compiles is that same wrapper carrying the
        // ones this build assumes. The two differ on purpose: at cache-split
        // time the site's link graph does not exist yet, so a fingerprint over
        // the real value would check a page against something the build has not
        // computed. What it was actually compiled with is recorded separately
        // (`Outputs::backlinks`) and verified once the graph is known.
        let bare = self.bound(page, &rooted, &dir, template, &Backlinks::Off);
        let fingerprint = Hash::of_bytes(bare.as_bytes());
        let text = match self.backlinks.of(page).is_empty() {
            true => bare,
            false => self.bound(page, &rooted, &dir, template, &self.backlinks),
        };
        Ok((id, text, fingerprint))
    }

    /// The synthetic module binding `page` to the template `file` under `dir`.
    ///
    /// Split out from [`Prepare::input`] because a page is bound to a template
    /// more than once: to its layout for the HTML compile, and to a paged
    /// template for each sidecar that wraps the page (the PDF). They must hand
    /// the template the same `page` dict, or `page.strings` means one thing on
    /// screen and another on paper.
    ///
    /// Backlinks are the one exception, and are always empty here: a sidecar is
    /// a paged compile of the page, and a card or a printed PDF has nothing to
    /// click. Redrawing every sidecar whenever a page gained an inbound link
    /// would also make the repair pass cost what the whole build does.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn bind(
        &self,
        page: &Page,
        rooted: &RootedPath,
        dir: &str,
        file: &str,
    ) -> String {
        self.bound(page, rooted, dir, file, &Backlinks::Off)
    }

    /// [`Prepare::bind`] against a given set of backlinks: the one place the
    /// wrapper text is assembled, so the fingerprinted spelling and the compiled
    /// one can differ in that value alone and in nothing else.
    fn bound(
        &self,
        page: &Page,
        rooted: &RootedPath,
        dir: &str,
        file: &str,
        backlinks: &Backlinks,
    ) -> String {
        let vpath = Self::rooted_str(rooted);
        let body = match &page.data {
            Data::Generated(_) => Body::Inline(&page.body),
            _ => Body::Include,
        };
        self.with(page, backlinks, |context| {
            Layout::new(dir, file, &vpath, context, body).to_string()
        })
    }

    /// The `page` dict this page is handed as, with its frontmatter spelled as
    /// `frontmatter`: what a bundled document gives each of its entries, where
    /// there is no wrapper module per page to hold the binding.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn dict(&self, page: &Page, frontmatter: &str) -> String {
        self.with(page, &Backlinks::Off, |context| context.dict(&frontmatter))
    }

    /// Build this page's [`Context`] and hand it to `f`.
    ///
    /// The pieces borrow from locals, so they cannot be returned; taking the
    /// consumer instead keeps every caller reading one construction rather than
    /// each assembling its own and drifting.
    fn with<T>(&self, page: &Page, backlinks: &Backlinks, f: impl FnOnce(Context<'_>) -> T) -> T {
        let taxonomies = Typst(&page.taxonomies()).to_string();
        // prev/next sibling links, exposed to the template as `page.nav`. Part of
        // the wrapper text, so a neighbour's addition, removal, or retitling
        // refingerprints this page and rebuilds it: the cache stays correct.
        let nav = Typst(&Self::nav(&page.siblings)).to_string();
        let translations = Typst(&Self::translations(page)).to_string();
        let strings = Typst(&self.strings(&page.lang)).to_string();
        // Derived from this page's own body, so it names nothing outside the
        // page and cannot widen the fingerprint the way a site-wide value would.
        let reading = Typst(&Self::reading(&page.body)).to_string();
        // Names only the pages that link *here*, the same line `nav` sits on: a
        // page's neighbours, never the site. What keeps it out of the page's
        // fingerprint is in [`Prepare::input`], not here.
        let backlinks = Typst(&backlinks.value(page)).to_string();
        let date = Typst(&self.date(page)).to_string();
        let bind = match &page.data {
            Data::Export => Bind::Import,
            Data::Empty => Bind::Literal("(:)"),
            Data::Generated(dict) => Bind::Literal(dict),
        };
        f(Context {
            data: bind,
            taxonomies: &taxonomies,
            nav: &nav,
            lang: &page.lang,
            translations: &translations,
            strings: &strings,
            reading: &reading,
            backlinks: &backlinks,
            date: &date,
        })
    }

    /// One language's [`Section`] tree as a value: written out by [`Generated`]
    /// for that language's templates to import (the single source a site nav is
    /// built from, so it can't drift from the pages) and reused by the
    /// `baudelaire:sections` JS module. Each node is
    /// `(id, pages: ((url, title), ..), children: (..))`, one per content
    /// directory; generated listings are excluded.
    pub(in crate::engine) fn sections(&self, lang: &str) -> Value {
        Value::array(
            Section::tree(self.pages, self.config, lang)
                .iter()
                .map(Section::value),
        )
    }

    /// The import root a template is loaded from, layout or paged alike.
    ///
    /// The project's own template directory, expressed relative to the root
    /// because a typst import is root-absolute in the compiler's terms, not the
    /// config's, and resolved once for the build rather than per page. A
    /// template the project does not have falls back to the theme's package,
    /// which the compiler resolves by spec rather than by path.
    pub(in crate::engine) fn dir(&self, template: &str) -> String {
        match self.theme {
            Some(theme)
                if !self.config.paths.templates.join(template).is_file()
                    && theme.has_template(template) =>
            {
                theme.templates()
            }
            _ => format!("/{}", self.templates.display()),
        }
    }

    /// The prev/next sibling links as a typst dict value:
    /// `(prev: (url: .., title: ..), next: none)`. Each link is a dict or `none`,
    /// so a template reads `page.nav.prev.url` / `page.nav.next` uniformly.
    fn nav(siblings: &Siblings) -> Value {
        let link = |s: &Option<Sibling>| match s {
            Some(s) => Value::dict([("url", Value::str(&s.url)), ("title", Value::str(&s.title))]),
            None => Value::None,
        };
        Value::dict([
            ("prev", link(&siblings.prev)),
            ("next", link(&siblings.next)),
        ])
    }

    /// A page's reading estimate as a typst dict value:
    /// `(words: 1200, minutes: 6)`, exposed to the template as `page.reading`
    /// for a "6 min read" badge.
    fn reading(body: &str) -> Value {
        let reading = crate::engine::text::Reading::of(body);
        Value::dict([
            (
                "words",
                Value::Int(i64::try_from(reading.words).unwrap_or(i64::MAX)),
            ),
            (
                "minutes",
                Value::Int(i64::try_from(reading.minutes).unwrap_or(i64::MAX)),
            ),
        ])
    }

    /// A page's date in both forms: `(iso: .., display: ..)`, or `none` when it
    /// carries no date.
    ///
    /// The ISO day is what a `<time datetime>` wants and the localized one is
    /// what a reader wants, and a template cannot derive the second from the
    /// first: typst's `datetime.display` knows English month names only, so a
    /// French site rendered `2026-07-30` whatever its layout did.
    fn date(&self, page: &Page) -> Value {
        let strings = Strings::new(self.config, &page.lang);
        match page.frontmatter.date {
            None => Value::None,
            Some(date) => Value::dict([
                ("iso", Value::str(Iso(date).to_string())),
                (
                    "display",
                    Value::str(Localized::new(date, &strings).to_string()),
                ),
            ]),
        }
    }

    /// A page's translations as an array value:
    /// `((lang: .., url: .., title: ..), ..)`, exposed to the template as
    /// `page.translations` for a language switcher. Empty on a single-language
    /// site.
    fn translations(page: &Page) -> Value {
        Value::array(page.translations.iter().map(|t| {
            Value::dict([
                ("lang", Value::str(&t.lang)),
                ("url", Value::str(&t.url)),
                ("title", Value::str(&t.title)),
            ])
        }))
    }

    /// A language's UI-string table as a dict value, exposed to the template as
    /// `page.strings`. Empty for a language with no `strings` block.
    fn strings(&self, lang: &str) -> Value {
        Value::dict(
            self.config
                .strings(lang)
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        )
    }

    /// A page's project-root-absolute virtual path (`/content/posts/a.typ`):
    /// what the wrapper's `#import`/`#include` literals resolve against.
    fn rooted_str(rooted: &RootedPath) -> String {
        format!("/{}", rooted.vpath().get_without_slash())
    }

    /// The synthetic wrapper's file id: a sibling of the page (so relative
    /// template imports resolve the same way), but distinct from it, so the
    /// wrapper can `#include` the real file without shadowing it as `main`.
    fn wrapper(rooted: &RootedPath) -> FileId {
        let name = format!("{}@layout", rooted.vpath().get_without_slash());
        let vpath = VirtualPath::new(&name)
            .expect("a page vpath with a suffix stays a valid relative vpath");
        FileId::new(RootedPath::new(VirtualRoot::Project, vpath))
    }
}
