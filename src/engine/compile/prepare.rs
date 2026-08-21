//! The compile input for a page: the synthetic Typst module that binds it to
//! its template, and the values injected into that module.

use std::collections::BTreeMap;
use std::path::PathBuf;

use typst::syntax::{FileId, RootedPath};

use crate::codegen::Value;
use crate::config::Config;
use crate::content::{
    Byline, Data, Iso, Localized, Page, Registries, Relations, Section, Sibling, Siblings, Strings,
};
use crate::error::{Result, TemplateMissing};
use crate::git::History;
use crate::graph::Hash;
use crate::render::Backlinks;
use crate::theme::Theme;
use crate::ui::{Code, markup};
use crate::world::Project;
use crate::world::Wrapper;
use crate::world::module;

use super::layout::{Bind, Body, Context, Layout};
use crate::world::generated::Table;

/// A page reduced to what the cache check needs: its `FileId`, the exact text
/// typst will compile, and that text's fingerprint.
pub(in crate::engine) type Prepared = (FileId, String, Hash);

/// Builds every page's compile input against one build's shared state: the site
/// config, the project world, the resolved theme, and the section trees.
pub(in crate::engine) struct Prepare<'a> {
    config: &'a Config,
    /// The entity registries a page's credited references resolve against.
    entities: &'a Registries,
    /// For a page that *declares* an entity, the pages that name it: an
    /// author's own archive, handed to their profile as `page.members`.
    members: BTreeMap<PathBuf, Vec<Value>>,
    project: &'a Project,
    theme: Option<&'a Theme>,
    pages: &'a [Page],
    /// Where each page sits among the others, which the plan worked out once
    /// the whole page set was known.
    relations: &'a Relations,
    /// One section tree per built language, kept out of the wrapper text: it
    /// names every page, so it would tie every page's fingerprint to every
    /// other page's title and URL.
    trees: BTreeMap<String, Value>,
    /// The template directory as the compiler spells it, resolved once for the
    /// build.
    templates: PathBuf,
    /// The backlinks each page is compiled against: a *prediction* until the
    /// site has rendered, and [`Backlinks::Off`] until a build sets one.
    backlinks: Backlinks,
    /// What git knows about each page, empty unless `content { history }` is on.
    history: &'a History,
}

impl<'a> Prepare<'a> {
    pub(in crate::engine) fn new(
        config: &'a Config,
        project: &'a Project,
        theme: Option<&'a Theme>,
        pages: &'a [Page],
        entities: &'a Registries,
        relations: &'a Relations,
        history: &'a History,
    ) -> Self {
        let base = Self {
            config,
            entities,
            history,
            members: Self::members(config, entities, pages),
            project,
            theme,
            pages,
            relations,
            trees: BTreeMap::new(),
            templates: config.paths.under(project.root()).templates,
            backlinks: Backlinks::Off,
        };
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
    /// `None` where the page never saw them: the feature is off, or the page
    /// has no template and so no wrapper to carry the value.
    pub(in crate::engine) fn digest(&self, page: &Page) -> Option<Hash> {
        page.template.as_ref()?;
        self.backlinks.digest(page)
    }

    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn config(&self) -> &Config {
        self.config
    }

    /// Who `page` credits, with the site's own author as the floor.
    pub(in crate::engine) fn byline(&self, page: &Page) -> Byline {
        Byline::of(self.entities, self.config, page).or_site(self.config, page)
    }

    /// Which pages each described term holds, keyed by the profile page that
    /// describes it.
    ///
    /// A term whose slug is empty or collides is an error the plan already
    /// raised, so a group that will not resolve contributes nothing rather than
    /// failing a second time.
    fn members(
        config: &'a Config,
        entities: &'a Registries,
        pages: &'a [Page],
    ) -> BTreeMap<PathBuf, Vec<Value>> {
        let mut members: BTreeMap<PathBuf, Vec<Value>> = BTreeMap::new();
        for group in crate::content::Taxonomy::groups(config, entities, pages) {
            let strings = Strings::new(config, group.lang());
            for term in group.resolve().unwrap_or_default() {
                let Some(profile) = term.described else {
                    continue;
                };
                members
                    .entry(crate::fs::resolved(&profile.source))
                    .or_default()
                    .extend(
                        term.members.iter().map(|member| {
                            crate::content::listing::Item::of(member, &strings).value()
                        }),
                    );
            }
        }
        members
    }

    /// The files templates import, ready to write: the section tree and the
    /// page catalogue.
    ///
    /// Sized by the file-backed module registry, so a module added there
    /// without a table to write it fails to compile.
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

    /// The compile input for a page: its (possibly synthetic) source and a
    /// fingerprint of the exact text typst compiles.
    ///
    /// The fingerprint is taken over the wrapper as it would read with *no*
    /// backlinks, since the link graph does not exist at cache-split time; what
    /// the page was compiled with is recorded separately (`Outputs::backlinks`)
    /// and verified once the graph is known. A page with no template has no
    /// wrapper, so its byline is folded in by hand: the roster is read at plan
    /// time and is in no page's dependency set.
    pub(in crate::engine) fn input(&self, page: &Page) -> Result<Prepared> {
        let rooted = self.project.virtualize(&page.source)?;
        let Some(template) = &page.template else {
            let text = page.body.clone();
            let fingerprint = Hash::of(&(&text, Value::from(&self.byline(page))));
            return Ok((FileId::new(rooted), text, fingerprint));
        };
        let id = match &page.data {
            Data::Generated { .. } => FileId::new(rooted.clone()),
            _ => Wrapper::id(&rooted),
        };
        let dir = self.dir(template);
        let bare = self.bound(page, &rooted, &dir, template, &Backlinks::Off);
        let fingerprint = Hash::of_bytes(bare.as_bytes());
        let text = if self.backlinks.of(page).is_empty() {
            bare
        } else {
            self.bound(page, &rooted, &dir, template, &self.backlinks)
        };
        Ok((id, text, fingerprint))
    }

    /// The synthetic module binding `page` to the template `file` under `dir`,
    /// with no backlinks: a paged artifact has nothing to click.
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
    /// wrapper text is assembled, so the fingerprinted spelling and the
    /// compiled one can differ in that value alone.
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
            #[cfg(feature = "markdown")]
            Data::Lowered { .. } => Body::Inline(&page.body),
            Data::Generated { .. } => Body::Inline(&page.body),
            _ => Body::Include,
        };
        let context = self.context(page, backlinks);
        Layout::new(dir, file, &vpath, context, body).to_string()
    }

    /// The `page` dict this page is handed as, with its frontmatter spelled as
    /// `frontmatter`: what a bundled document gives each of its entries, where
    /// there is no wrapper module per page to hold the binding.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn dict(&self, page: &Page, frontmatter: &str) -> String {
        let context = self.context(page, &Backlinks::Off);
        crate::codegen::Typst(&context.dict(Value::Raw(frontmatter.to_owned()))).to_string()
    }

    /// This page's [`Context`]: every value a template is handed, read off the
    /// page and the plan around it.
    fn context(&self, page: &Page, backlinks: &Backlinks) -> Context {
        Context {
            data: match &page.data {
                Data::Export => Bind::Import,
                Data::Empty => Bind::Literal("(:)".to_owned()),
                #[cfg(feature = "markdown")]
                Data::Lowered { dict, .. } => Bind::Literal(dict.clone()),
                Data::Generated { dict, .. } => Bind::Literal(dict.clone()),
            },
            taxonomies: page.taxonomies(),
            credits: Value::from(&self.byline(page)),
            members: Value::array(
                self.members
                    .get(&crate::fs::resolved(&page.source))
                    .cloned()
                    .unwrap_or_default(),
            ),
            nav: Self::nav(&self.relations.of(page).siblings),
            lang: Value::str(&page.lang),
            translations: self.translations(page),
            strings: self.strings(&page.lang),
            reading: self.reading(page),
            backlinks: backlinks.value(page),
            date: self.date(page),
            url: Value::str(self.config.prefixed(&page.permalink)),
            collection: Value::str(page.section()),
            assets: self.colocated(page),
            source: self.source(page),
            git: self.history(page),
            defaults: self.defaults(page),
        }
    }

    /// What this page's collection schema fills in for a field the page left
    /// out, as a dict the wrapper lays under whatever the page wrote.
    ///
    /// Empty where the schema declares none, which leaves the wrapper of every
    /// page on such a site exactly what it was.
    fn defaults(&self, page: &Page) -> Value {
        Value::dict(
            self.config
                .schema(&page.collection)
                .iter()
                .filter_map(|(key, field)| Some((key.clone(), field.default.clone()?))),
        )
    }

    /// What git knows about this page: the commit that last changed it, and
    /// everyone who has where `content { history { contributors } }` asked.
    ///
    /// `None` with the feature off, for a generated listing, and for a page no
    /// commit has ever touched.
    fn history(&self, page: &Page) -> Value {
        if !page.authored() {
            return Value::None;
        }
        let key = crate::graph::Portable(self.project.root()).key(&page.source);
        self.history.of(&key).map_or(Value::None, Value::from)
    }

    /// The page's own file as the compiler spells it, for a template that has
    /// to name it: an edit link, a provenance line.
    ///
    /// `None` for a generated listing. Its path is synthetic and names no file
    /// on disk, so a link built from one leads nowhere.
    fn source(&self, page: &Page) -> Value {
        if !page.authored() {
            return Value::None;
        }
        self.project
            .virtualize(&page.source)
            .map_or(Value::None, |rooted| Value::str(Self::rooted_str(&rooted)))
    }

    /// The files sitting beside a *page bundle*, as authored name to served
    /// URL, spelled the way the image pipeline publishes an extracted file.
    ///
    /// Empty for a page that shares its directory with its neighbours
    /// (`posts/hello.typ`), whose siblings are not its own. Every extension
    /// [`Config::sources`] names is skipped: a page is not an asset, whichever
    /// dialect it is written in, and one listed here is a URL nothing
    /// publishes.
    fn colocated(&self, page: &Page) -> crate::codegen::Value {
        use crate::codegen::Value;
        let Some(dir) = page.source.parent() else {
            return Value::dict::<&str>([]);
        };
        let bundled = page
            .source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem == self.config.index());
        if !bundled || !page.authored() {
            return Value::dict::<&str>([]);
        }
        let root = crate::fs::canonical(&self.config.root);
        let content = crate::fs::canonical(&self.config.paths.content);
        let sources = self.config.sources();
        let is_page = |path: &std::path::Path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| sources.contains(&ext))
        };
        let mut entries: Vec<(String, Value)> = Vec::new();
        let Ok(read) = std::fs::read_dir(dir) else {
            return Value::dict::<&str>([]);
        };
        for file in read.flatten() {
            let path = file.path();
            if !path.is_file() || is_page(&path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let rel = crate::fs::canonical(&path);
            let rel = rel
                .strip_prefix(&content)
                .unwrap_or_else(|_| rel.strip_prefix(&root).unwrap_or(&rel));
            let named = crate::graph::AssetName::new(rel, self.config.digest_of(&path)).path();
            entries.push((name.to_owned(), Value::str(self.config.asset_url(&named))));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Value::dict(entries)
    }

    /// One language's [`Section`] tree as a value: each node is
    /// `(id, pages: ((url, title), ..), children: (..))`, one per content
    /// directory, with generated listings excluded.
    pub(in crate::engine) fn sections(&self, lang: &str) -> Value {
        Value::array(
            Section::tree(self.pages, self.config, lang)
                .iter()
                .map(Section::value),
        )
    }

    /// Every template this build will import, and what named it: each page's
    /// resolved layout, then the paged templates the config asks for. Deduped
    /// by filename and attributed to the first page that named it, so a missing
    /// layout is reported once however many pages bind it.
    fn asked(&self) -> Vec<(String, String)> {
        let mut asked: Vec<(String, String)> = Vec::new();
        let mut push = |file: &str, by: String| {
            if !asked.iter().any(|(f, _)| f == file) {
                asked.push((file.to_owned(), by));
            }
        };
        for page in self.pages {
            if let Some(template) = &page.template {
                push(template, markup!("`{}`", page.source.display().to_string()));
            }
        }
        let cards = &self.config.artifacts.cards;
        if cards.active() {
            push(&cards.template, "`artifacts { cards }`".to_owned());
        }
        let config = &self.config;
        let pdf = &config.artifacts.pdf;
        if pdf.pages.active() {
            push(
                &pdf.pages.template,
                "`artifacts { pdf { pages } }`".to_owned(),
            );
        }
        for (id, bundle) in &config.artifacts.bundles {
            if bundle.active().contains(&crate::config::BundleFormat::Pdf) {
                push(
                    &bundle.template,
                    markup!("`artifacts {{ bundles {{ {} }} }}`", Code(id)),
                );
            }
        }
        asked
    }

    /// Fail on a template nothing supplies, before the first compile, so it is
    /// reported once and against what asked for it rather than once per page
    /// against a generated wrapper.
    pub(in crate::engine) fn verify(&self) -> Result<()> {
        let searched: Vec<String> = self
            .theme
            .map(|theme| theme.templates().trim_start_matches('/').to_owned())
            .into_iter()
            .chain([self.config.paths.templates.display().to_string()])
            .collect();
        for (file, asked) in self.asked() {
            if !self.supplied(&file) {
                return Err(TemplateMissing::new(&file, &asked, &searched).into());
            }
        }
        Ok(())
    }

    /// Whether any layer carries `template`: the project's directory, else the
    /// theme's, in the order [`Self::dir`] resolves in.
    fn supplied(&self, template: &str) -> bool {
        self.config.paths.templates.join(template).is_file()
            || self.theme.is_some_and(|theme| theme.has_template(template))
    }

    /// The import root a template is loaded from, layout or paged alike: the
    /// project's own directory as a root-absolute typst path, else the theme's
    /// package spec.
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
    /// `(prev: (url: .., title: ..), next: none)`, each link a dict or `none`.
    fn nav(siblings: &Siblings) -> Value {
        let link = |s: &Option<Sibling>| {
            s.as_ref().map_or(Value::None, |s| {
                Value::dict([("url", Value::str(&s.url)), ("title", Value::str(&s.title))])
            })
        };
        Value::dict([
            ("prev", link(&siblings.prev)),
            ("next", link(&siblings.next)),
        ])
    }

    /// A page's reading estimate as a typst dict value:
    /// `(words: 1200, minutes: 6)`.
    ///
    /// A page whose body is *generated* Typst carries an estimate of its own,
    /// measured on the text its author wrote: reading the generated body would
    /// count machinery rather than prose.
    fn reading(&self, page: &Page) -> Value {
        let reading = match &page.data {
            #[cfg(feature = "markdown")]
            Data::Lowered { reading, .. } => *reading,
            _ => crate::engine::text::Reading::of(&page.body),
        };
        let minutes = reading.minutes(self.config.wpm(&page.lang));
        Value::dict([
            (
                "words",
                Value::Int(i64::try_from(reading.words).unwrap_or(i64::MAX)),
            ),
            (
                "minutes",
                Value::Int(i64::try_from(minutes).unwrap_or(i64::MAX)),
            ),
        ])
    }

    /// A page's date in both forms: `(iso: .., display: ..)`, or `none` when it
    /// carries no date; typst's `datetime.display` knows English month names
    /// only, so a template cannot derive the localized form from the ISO one.
    fn date(&self, page: &Page) -> Value {
        let strings = Strings::new(self.config, &page.lang);
        page.frontmatter.date.map_or(Value::None, |date| {
            Value::dict([
                ("iso", Value::str(Iso(date).to_string())),
                (
                    "display",
                    Value::str(Localized::new(date, &strings).to_string()),
                ),
            ])
        })
    }

    /// A page's translations as an array value:
    /// `((lang: .., url: .., title: ..), ..)`. Empty on a single-language site.
    fn translations(&self, page: &Page) -> Value {
        Value::array(self.relations.of(page).translations.iter().map(|t| {
            Value::dict([
                ("lang", Value::str(&t.lang)),
                ("url", Value::str(&t.url)),
                ("title", Value::str(&t.title)),
            ])
        }))
    }

    /// A language's UI-string table as a dict value. Empty for a language with
    /// no `strings` block.
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
}
