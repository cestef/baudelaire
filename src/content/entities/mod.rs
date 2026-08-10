//! The named things a page refers to, and what is known about each.
//!
//! A page credits a person, belongs to a series, is published by an
//! organization. All three are the same shape: a term the page writes, an id in
//! a registry, and a set of fields the build can render. This module is that
//! shape; nothing in it is about people.
//!
//! [`Registries`] is built once per plan, from the
//! [`entities`](crate::config::RegistryConfig) block: each registry loads its
//! [`sources`](source) in order, merges what they declare, holds every entity to
//! the fields the registry says they carry, and answers `who is "zoe"`. What
//! *asks* is a taxonomy naming a registry, so the terms of `authors` are people
//! while the terms of `tags` stay words.

pub mod credit;
pub mod provenance;
pub mod source;

use std::collections::BTreeMap;

use crate::codegen::Value;
use crate::config::{Config, RegistryConfig, Slots, Unknown};
use crate::content::frontmatter::check::{Check, Fault, Step};
use crate::content::{Page, Slug};
use crate::error::{EntityError, Result, entity::Unresolved};
use crate::ui::Ui;
use crate::world::Project;

pub use credit::{Attribution, Byline, Credit, Resolved, Vocabulary};
pub use provenance::{Provenance, Snippet};
use source::SourceCtx;

/// One entity: an id, the other names that resolve to it, and its fields.
#[derive(Debug, Clone)]
pub struct Entity {
    id: String,
    aliases: Vec<String>,
    fields: Vec<(String, Value)>,
    from: Provenance,
    /// The fields each language's edition declares, over the base ones.
    ///
    /// One entity, several editions: a profile page has an edition per language
    /// exactly as any other page does, and a French post credits the same
    /// person an English one does. Without this the merge kept one edition's
    /// name and every language rendered it, so a French byline read `Zoe` while
    /// the page beside it read `Zoé`.
    editions: BTreeMap<String, Vec<(String, Value)>>,
    /// The page that declares this entity, per language.
    ///
    /// A map and not one path, because a profile has an edition per language
    /// exactly as any other page does, and the two editions are one entity: a
    /// French post credits the same person an English one does. Which page
    /// *describes* a term is therefore a question with one answer per language,
    /// and asking it without one silently answered every language with the
    /// first edition read.
    pages: BTreeMap<String, std::path::PathBuf>,
}

impl Entity {
    /// The field naming the other spellings that resolve to this entity.
    ///
    /// Read out of the fields rather than declared beside them, so every source
    /// carries aliases without a shape of its own: a page writes
    /// `alias: ("cstef",)`, a roster writes `alias "cstef"`, and neither
    /// source has to know what the key means.
    pub const ALIAS: &'static str = "alias";

    /// An entity built from what a source read: an already-slugged id, and the
    /// fields under it, with the aliases lifted out.
    pub fn new(id: &str, fields: Vec<(String, Value)>, from: Provenance) -> Result<Self> {
        Self::declared(id, fields, from, BTreeMap::new(), BTreeMap::new())
    }

    /// An entity a page declared, in that page's own language.
    pub fn authored(
        id: &str,
        fields: Vec<(String, Value)>,
        from: Provenance,
        lang: &str,
        page: std::path::PathBuf,
    ) -> Result<Self> {
        Self::declared(
            id,
            fields.clone(),
            from,
            BTreeMap::from([(lang.to_owned(), fields)]),
            BTreeMap::from([(lang.to_owned(), page)]),
        )
    }

    fn declared(
        id: &str,
        mut fields: Vec<(String, Value)>,
        from: Provenance,
        editions: BTreeMap<String, Vec<(String, Value)>>,
        pages: BTreeMap<String, std::path::PathBuf>,
    ) -> Result<Self> {
        let aliases = match fields.iter().position(|(key, _)| key == Self::ALIAS) {
            Some(at) => Self::names(fields.remove(at).1),
            None => Vec::new(),
        };
        let aliases = aliases
            .iter()
            .map(|alias| Ok(Slug::require(alias)?.into_string()))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            id: Slug::require(id)?.into_string(),
            aliases,
            fields,
            from,
            editions,
            pages,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    pub fn from(&self) -> &Provenance {
        &self.from
    }

    /// One field, by name, as `lang` declares it.
    ///
    /// The language's own edition wins, and falls back to the base fields for
    /// anything it did not declare: a French profile that writes only a name
    /// still carries the homepage the roster gave it.
    pub fn field(&self, key: &str, lang: &str) -> Option<&Value> {
        Self::look(self.editions.get(lang), key).or_else(|| Self::look(Some(&self.fields), key))
    }

    /// Every field as `lang` declares it, the edition's over the base ones, in
    /// the order each was first declared.
    pub fn fields(&self, lang: &str) -> Vec<(&str, &Value)> {
        let edition = self.editions.get(lang);
        self.fields
            .iter()
            .map(|(key, _)| key.as_str())
            .chain(edition.into_iter().flatten().map(|(key, _)| key.as_str()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter_map(|key| Some((key, self.field(key, lang)?)))
            .collect()
    }

    /// One field of one set.
    fn look<'a>(fields: Option<&'a Vec<(String, Value)>>, key: &str) -> Option<&'a Value> {
        fields?
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// Fill from a source read later: what `self` already carries wins, and
    /// what it lacks is taken.
    ///
    /// The [`Section::fill`](crate::config::dispatch) policy, applied to an
    /// entity: a roster file can carry an email while a profile page carries
    /// the name, and declaring one does not silently drop the other.
    fn fill(&mut self, other: Self) {
        for (key, value) in other.fields {
            if Self::look(Some(&self.fields), &key).is_none() {
                self.fields.push((key, value));
            }
        }
        for (lang, fields) in other.editions {
            let edition = self.editions.entry(lang).or_default();
            for (key, value) in fields {
                if Self::look(Some(edition), &key).is_none() {
                    edition.push((key, value));
                }
            }
        }
        for alias in other.aliases {
            if !self.aliases.contains(&alias) {
                self.aliases.push(alias);
            }
        }
        // Every language's page, not just the first read: a later source may be
        // the one that declares the French edition, and which source came first
        // must not decide whether a term has a page in a given language.
        for (lang, page) in other.pages {
            self.pages.entry(lang).or_insert(page);
        }
    }

    /// The language whose edition a lookup reads when there is no edition for
    /// the language asked about: the base fields, as every source but `pages`
    /// declares them.
    ///
    /// Not a real language code, and it cannot be one: a source that is not a
    /// page has no language at all.
    pub const BASE: &'static str = "";

    /// Every language this entity has an edition for.
    pub fn langs(&self) -> impl Iterator<Item = &str> {
        self.editions.keys().map(String::as_str)
    }

    /// The page that declares this entity in `lang`, if one does.
    pub fn page(&self, lang: &str) -> Option<&std::path::Path> {
        self.pages.get(lang).map(std::path::PathBuf::as_path)
    }

    /// A string list value, however it was written: one name or several.
    fn names(value: Value) -> Vec<String> {
        match value {
            Value::Str(one) => vec![one],
            Value::Array(many) => many
                .into_iter()
                .filter_map(|item| match item {
                    Value::Str(name) => Some(name),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// One registry: its entities, the names that reach them, and what the site
/// declared about all of them.
#[derive(Debug, Clone)]
pub struct Registry {
    id: String,
    shape: Option<crate::config::Shape>,
    slots: Slots,
    unknown: Unknown,
    /// Entities by id, in id order so every walk of a registry is stable.
    entities: BTreeMap<String, Entity>,
    /// Alias to id, for the names that are not the entity's own.
    aliases: BTreeMap<String, String>,
}

impl Registry {
    /// Load a registry: every source in order, merged, checked, indexed.
    fn build(id: &str, config: &RegistryConfig, cx: &SourceCtx<'_>) -> Result<Self> {
        let mut entities: BTreeMap<String, Entity> = BTreeMap::new();
        for source in &config.sources {
            for entity in source.loader().load(cx)? {
                match entities.get_mut(entity.id()) {
                    Some(known) => known.fill(entity),
                    None => {
                        entities.insert(entity.id().to_owned(), entity);
                    }
                }
            }
        }
        let mut registry = Self {
            id: id.to_owned(),
            shape: config.shape,
            slots: config.slots.clone(),
            unknown: config.unknown(),
            entities,
            aliases: BTreeMap::new(),
        };
        registry.check(config, cx.project)?;
        registry.index(cx.project)?;
        Ok(registry)
    }

    /// Hold every entity to the fields the registry says its entities carry.
    ///
    /// Through the very checker a collection's frontmatter schema goes through,
    /// so a `list<dict>` means the same thing in both places and a mismatch
    /// reads the same way whichever declared it. The fault is underlined where
    /// the entity was written, whichever source that was.
    fn check(&self, config: &RegistryConfig, project: &Project) -> Result<()> {
        if config.fields.is_empty() {
            return Ok(());
        }
        for entity in self.entities.values() {
            // Every edition, not just the base: a French profile that omits a
            // field the registry requires is as broken as an English one, and
            // it is the edition a French page renders.
            for lang in entity.langs() {
                let dict = entity
                    .fields(lang)
                    .into_iter()
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect();
                if let Some(fault) = Check::default().dict(&config.fields, &dict) {
                    let steps = match &fault {
                        Fault::Missing { .. } => fault.parent(),
                        Fault::Mismatch { .. } => fault.path(),
                    };
                    let snippet = entity.from().snippet(project, steps);
                    return Err(EntityError::field(&self.id, entity, &fault, snippet).into());
                }
            }
            let dict = entity
                .fields(Entity::BASE)
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect();
            if let Some(fault) = Check::default().dict(&config.fields, &dict) {
                // The same rule a page's schema failure follows: a mismatch
                // underlines the value, and a missing field whatever should
                // have held it, since what is absent has no place of its own.
                let steps = match &fault {
                    Fault::Missing { .. } => fault.parent(),
                    Fault::Mismatch { .. } => fault.path(),
                };
                let snippet = entity.from().snippet(project, steps);
                return Err(EntityError::field(&self.id, entity, &fault, snippet).into());
            }
        }
        Ok(())
    }

    /// Build the alias index, refusing a name that would reach two entities.
    ///
    /// Both directions matter: two entities claiming one alias is a name with
    /// no answer, and an alias equal to another entity's id is worse, because
    /// it resolves in silence to whichever the lookup happens to try first.
    fn index(&mut self, project: &Project) -> Result<()> {
        for entity in self.entities.values() {
            for alias in entity.aliases() {
                let clash = match self.entities.get(alias) {
                    Some(other) => Some(other.id()),
                    None => self.aliases.get(alias).map(String::as_str),
                };
                if let Some(first) = clash {
                    // Underlined at the entity that claimed the alias second,
                    // which is the one an author is looking at.
                    let snippet = entity.from().snippet(project, &[]);
                    return Err(
                        EntityError::alias(&self.id, alias, first, entity.id(), snippet).into(),
                    );
                }
                self.aliases.insert(alias.clone(), entity.id().to_owned());
            }
        }
        Ok(())
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Which field answers each question a renderer asks here.
    pub fn slots(&self) -> &Slots {
        &self.slots
    }

    /// The named field set this registry took, if it took one: what says
    /// whether its entities are people or something else.
    pub fn shape(&self) -> Option<crate::config::Shape> {
        self.shape
    }

    /// What a reference nobody declared means here.
    pub fn unknown(&self) -> Unknown {
        self.unknown
    }

    /// The entity a term names: its own id, else an alias of one.
    pub fn get(&self, term: &str) -> Option<&Entity> {
        let id = Slug::parse(term)?.into_string();
        self.entities
            .get(&id)
            .or_else(|| self.aliases.get(&id).and_then(|id| self.entities.get(id)))
    }

    /// The page that declares the entity a term names, in `lang`.
    ///
    /// What makes a term describable: an entity written as a profile page has a
    /// page of its own to be the term's, while one written in a roster has only
    /// fields. Per language, so a French term is described by the French
    /// edition or by nothing.
    pub fn page(&self, term: &str, lang: &str) -> Option<&std::path::Path> {
        self.get(term)?.page(lang)
    }

    /// The id `term` resolves to: its own, or the entity an alias reaches.
    ///
    /// What every consumer that *groups* by term has to call first. Grouping on
    /// the raw string splits one person across two terms the moment a page
    /// spells them by an alias, which is two term pages, two feeds and two rows
    /// in the index for one entity.
    pub fn canonical<'a>(&'a self, term: &'a str) -> &'a str {
        self.get(term).map_or(term, Entity::id)
    }

    /// Every name that resolves here, ids and aliases alike: what a
    /// did-you-mean is drawn from.
    fn names(&self) -> Vec<&str> {
        self.entities
            .keys()
            .chain(self.aliases.keys())
            .map(String::as_str)
            .collect()
    }
}

/// Every registry a site declared, and the taxonomies that reference them.
#[derive(Debug, Clone, Default)]
pub struct Registries(BTreeMap<String, Registry>);

impl Registries {
    /// Build every registry from its sources.
    ///
    /// Runs inside [`crate::content::plan`], after discovery: a `pages` source
    /// draws its entities from pages the plan has already read, so no file is
    /// opened twice and a profile page is an ordinary page in every other way.
    pub fn build(config: &Config, project: &Project, pages: &[Page]) -> Result<Self> {
        let mut registries = BTreeMap::new();
        for (id, registry) in &config.content.entities {
            let cx = SourceCtx {
                config,
                project,
                pages,
                registry: id,
            };
            registries.insert(id.clone(), Registry::build(id, registry, &cx)?);
        }
        let registries = Self(registries);
        registries.referenced(config)?;
        Ok(registries)
    }

    /// Refuse a taxonomy naming a registry nobody declared, before anything
    /// tries to resolve a term into it.
    fn referenced(&self, config: &Config) -> Result<()> {
        let names: Vec<&str> = self.0.keys().map(String::as_str).collect();
        for (taxonomy, cfg) in &config.content.taxonomies {
            let Some(registry) = &cfg.entities else {
                continue;
            };
            if !self.0.contains_key(registry) {
                return Err(EntityError::no_registry(taxonomy, registry, &names).into());
            }
        }
        Ok(())
    }

    /// The empty set: a site that declared no registry, and what a consumer
    /// assembled outside a plan reads.
    ///
    /// Borrowed rather than constructed, because every reader holds a
    /// reference: the alternative is each of them owning an empty map of its
    /// own for the life of the build.
    pub fn none() -> &'static Self {
        static NONE: std::sync::LazyLock<Registries> =
            std::sync::LazyLock::new(Registries::default);
        &NONE
    }

    /// One registry, by id.
    pub fn get(&self, id: &str) -> Option<&Registry> {
        self.0.get(id)
    }

    /// Resolve every reference every page writes, under each registry's own
    /// policy for a term nobody declared.
    ///
    /// Separate from [`Registries::build`] and given the [`Ui`], because two of
    /// the three policies do not fail: a site can ask to be *told* that a page
    /// credits somebody who is not in the roster, and a site with no roster
    /// means the term as written and is told nothing at all.
    pub fn check(&self, config: &Config, project: &Project, pages: &[Page], ui: &Ui) -> Result<()> {
        if !self.any() {
            return Ok(());
        }
        for page in pages.iter().filter(|page| page.authored()) {
            for reference in self.references(config, page) {
                if reference.entity().is_some() {
                    continue;
                }
                match reference.registry.unknown() {
                    // The term is the entity: nothing was ever declared about
                    // it, and nothing claimed otherwise.
                    Unknown::Synthesize => {}
                    Unknown::Warn => ui.warn(reference.unresolved(page, project)),
                    Unknown::Error => {
                        return Err(EntityError::from(reference.unresolved(page, project)).into());
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether a site declared any registry at all: what lets every consumer
    /// skip the whole mechanism on a site that never asked for it.
    pub fn any(&self) -> bool {
        !self.0.is_empty()
    }

    /// Every term of `page` that names an entity.
    ///
    /// The single reader of the taxonomy-to-registry wiring, so the check
    /// below and everything that renders a reference agree on which terms are
    /// ids.
    pub fn references<'a>(
        &'a self,
        config: &'a Config,
        page: &'a Page,
    ) -> impl Iterator<Item = Reference<'a>> {
        config
            .content
            .taxonomies
            .iter()
            .filter_map(|(taxonomy, cfg)| {
                let registry = self.get(cfg.entities.as_deref()?)?;
                let terms = page.frontmatter.taxonomies.get(&cfg.key)?;
                Some(
                    terms
                        .iter()
                        .enumerate()
                        .map(move |(index, term)| Reference {
                            taxonomy,
                            key: &cfg.key,
                            index,
                            registry,
                            term,
                            credit: cfg.credit,
                        }),
                )
            })
            .flatten()
    }
}

/// One term of one page, against the registry its taxonomy names.
pub struct Reference<'a> {
    /// The taxonomy the term was written under.
    pub taxonomy: &'a str,
    /// The frontmatter key it was written under, which is what locates it in
    /// the page: a taxonomy may read a key that is not its own id.
    pub key: &'a str,
    /// Which term of that key this is.
    pub index: usize,
    /// The registry its ids are drawn from.
    pub registry: &'a Registry,
    /// The term, as the page wrote it.
    pub term: &'a str,
    /// What the page claims about it, if the taxonomy says: `zoe` under
    /// `authors` wrote the page, while `rust` under `tags` claims nothing.
    pub credit: Option<Credit>,
}

impl Reference<'_> {
    /// The entity this term names, if the registry holds one.
    pub fn entity(&self) -> Option<&Entity> {
        self.registry.get(self.term)
    }

    /// This reference as the failure it is when nothing answers it, underlined
    /// at the term itself rather than at the page that carries it.
    ///
    /// At the severity the registry asked for, so `unknown "warn"` and
    /// `unknown "error"` report the same thing and differ only in whether the
    /// build survives it.
    pub fn unresolved(&self, page: &Page, project: &Project) -> Unresolved {
        let steps = [Step::Key(self.key.to_owned()), Step::Index(self.index)];
        let snippet = Provenance::Page {
            path: page.source.clone(),
        }
        .snippet(project, &steps);
        let unresolved = Unresolved::new(
            self.registry.id(),
            self.taxonomy,
            self.term,
            &self.registry.names(),
            snippet,
        );
        match self.registry.unknown() {
            Unknown::Warn => unresolved.lenient(),
            _ => unresolved,
        }
    }
}
