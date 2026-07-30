//! The compile input for a page: the synthetic Typst module that binds it to
//! its template, and the values injected into that module.
//!
//! Everything a page's wrapper text is made of lives here: which import root
//! the template comes from, the sibling and translation links, the UI strings.
//! They are wrapper *text*, so a change to any of them refingerprints the pages
//! that read it and the cache stays correct, which is why they are built once
//! for the whole site rather than per page.
//!
//! The section tree is the deliberate exception: it names every page on the
//! site, so as wrapper text it would tie every page's fingerprint to every
//! other page's title and URL. It goes to [`Sections`] instead, and reaches
//! templates as a file they import.
//!
//! [`Layout`] renders the text; this decides what goes into it.

use std::collections::BTreeMap;

use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};

use crate::codegen::{Typst, Value};
use crate::config::Config;
use crate::content::{Data, Iso, Localized, Page, Section, Sibling, Siblings, Strings};
use crate::error::Result;
use crate::graph::Hash;
use crate::theme::Theme;
use crate::world::Project;

use super::layout::{Bind, Body, Context, Layout};
use super::sections::Sections;

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
    /// templates as a file ([`Sections`]) and the JS bundler as a value, and
    /// putting it in the wrapper would make every page's fingerprint depend on
    /// every other page's title and URL.
    trees: BTreeMap<String, Value>,
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

    /// The site tree as the Typst source templates import, ready to write.
    pub(in crate::engine) fn tree_source(&self) -> Sections {
        Sections::new(self.trees.clone())
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
        let date = Typst(&self.date(page)).to_string();
        let vpath = Self::rooted_str(&rooted);
        let (id, bind, body) = match &page.data {
            Data::Export => (Self::wrapper(&rooted), Bind::Import, Body::Include),
            Data::Empty => (Self::wrapper(&rooted), Bind::Literal("(:)"), Body::Include),
            Data::Generated(dict) => (
                FileId::new(rooted.clone()),
                Bind::Literal(dict),
                Body::Inline(&page.body),
            ),
        };
        let context = Context {
            data: bind,
            taxonomies: &taxonomies,
            nav: &nav,
            lang: &page.lang,
            translations: &translations,
            strings: &strings,
            reading: &reading,
            date: &date,
        };
        let text = Layout::new(
            &self.template_root(template),
            template,
            &vpath,
            context,
            body,
        )
        .to_string();
        // hash the exact text typst compiles; the parse into a `Source` is
        // deferred to the compile, run only for stale pages.
        let fingerprint = Hash::of_bytes(text.as_bytes());
        Ok((id, text, fingerprint))
    }

    /// One language's [`Section`] tree as a value: written out by [`Sections`]
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

    /// The import root a page's layout is loaded from.
    ///
    /// The project's own template directory, expressed relative to the root
    /// because a typst import is root-absolute in the compiler's terms, not the
    /// config's. A template the project does not have falls back to the theme's
    /// package, which the compiler resolves by spec rather than by path.
    fn template_root(&self, template: &str) -> String {
        let project = self
            .config
            .paths
            .templates
            .strip_prefix(self.project.root())
            .unwrap_or(&self.config.paths.templates);
        match self.theme {
            Some(theme)
                if !self.config.paths.templates.join(template).is_file()
                    && theme.has_template(template) =>
            {
                theme.templates()
            }
            _ => format!("/{}", project.display()),
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
            ("words", Value::Int(reading.words as i64)),
            ("minutes", Value::Int(reading.minutes as i64)),
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
