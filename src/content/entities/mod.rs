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

pub use credit::{Attribution, Byline, Credit, EntityDeps, Resolved, Vocabulary};
pub use provenance::{Provenance, Snippet};
use source::SourceCtx;

/// One entity: an id, the other names that resolve to it, and its fields.
#[derive(Debug, Clone)]
pub struct Entity {
    id: String,
    aliases: Vec<String>,
    fields: Vec<(String, Value)>,
    from: Provenance,
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
    pub fn new(id: &str, mut fields: Vec<(String, Value)>, from: Provenance) -> Result<Self> {
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

    /// One field, by name.
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// Every field, in the order its first source declared it.
    pub fn fields(&self) -> &[(String, Value)] {
        &self.fields
    }

    /// What this entity says, as a digest: its id, its other names, and its
    /// fields.
    ///
    /// Not where it was declared. Moving an entity from a roster file into the
    /// config changes nothing a page renders, so it must not rebuild the pages
    /// that credit it.
    pub fn digest(&self) -> crate::graph::Hash {
        crate::graph::Hash::of(&(&self.id, &self.aliases, &self.fields))
    }

    /// Fill from a source read later: what `self` already carries wins, and
    /// what it lacks is taken.
    ///
    /// The [`Section::fill`](crate::config::dispatch) policy, applied to an
    /// entity: a roster file can carry an email while a profile page carries
    /// the name, and declaring one does not silently drop the other.
    fn fill(&mut self, other: Self) {
        for (key, value) in other.fields {
            if self.field(&key).is_none() {
                self.fields.push((key, value));
            }
        }
        for alias in other.aliases {
            if !self.aliases.contains(&alias) {
                self.aliases.push(alias);
            }
        }
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
            let dict = entity
                .fields()
                .iter()
                .map(|(key, value)| (key.as_str().into(), value.into()))
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

    /// The page that declared the entity a term names, if a page did.
    ///
    /// What makes a term describable: an entity written as a profile page has
    /// a page of its own to be the term's, while one written in a roster has
    /// only fields.
    pub fn page(&self, term: &str) -> Option<&std::path::Path> {
        match self.get(term)?.from() {
            Provenance::Page { path } => Some(path),
            Provenance::Roster { .. } => None,
        }
    }

    /// Every entity, in id order.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
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

    /// What the entity a probe named says now, `None` when nothing answers it.
    ///
    /// The read half of [`EntityDeps`](credit::EntityDeps): the cache records
    /// what a page consulted, and asks this whether it still says the same
    /// thing. Keyed by the term rather than by the resolved id, because an
    /// alias that stops resolving has to invalidate the page that used it.
    pub fn digest(&self, key: &str) -> Option<crate::graph::Hash> {
        let (registry, term) = key.split_once('/')?;
        self.get(registry)?.get(term).map(Entity::digest)
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
