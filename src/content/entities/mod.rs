//! The named things a page refers to, and what is known about each: a term the
//! page writes, an id in a registry, and a set of fields the build can render.
//!
//! [`Registries`] is built once per plan, from the
//! [`entities`](crate::config::RegistryConfig) block.

pub mod credit;
pub mod provenance;
pub mod source;

use std::collections::BTreeMap;

use crate::codegen::Value;
use crate::config::{Config, RegistryConfig, Slots, Unknown};
use crate::content::frontmatter::check::{Check, Step};
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
    editions: BTreeMap<String, Vec<(String, Value)>>,
    /// The page that declares this entity, one per language.
    pages: BTreeMap<String, std::path::PathBuf>,
}

impl Entity {
    /// The field naming the other spellings that resolve to this entity, read
    /// out of the fields so every source carries aliases without a shape of its
    /// own.
    pub const ALIAS: &'static str = "alias";

    /// An entity built from what a source read, with the aliases lifted out of
    /// its fields.
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
        let aliases = fields
            .iter()
            .position(|(key, _)| key == Self::ALIAS)
            .map_or_else(Vec::new, |at| Self::names(fields.remove(at).1));
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

    /// One field, by name, as `lang` declares it: the language's own edition
    /// wins, falling back to the base fields for anything it did not declare.
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
        for (lang, page) in other.pages {
            self.pages.entry(lang).or_insert(page);
        }
    }

    /// The language a lookup falls back to: the base fields, as every source
    /// but `pages` declares them. Not a real language code, and it cannot be
    /// one, since a source that is not a page has no language at all.
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

    /// Hold every entity, in every edition, to the fields the registry says its
    /// entities carry, through the checker a collection's frontmatter schema
    /// goes through.
    fn check(&self, config: &RegistryConfig, project: &Project) -> Result<()> {
        if config.fields.is_empty() {
            return Ok(());
        }
        for entity in self.entities.values() {
            for lang in entity.langs() {
                let dict = entity
                    .fields(lang)
                    .into_iter()
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect();
                if let Some(fault) = Check::default().dict(&config.fields, &dict) {
                    let snippet = entity.from().snippet(project, fault.steps());
                    return Err(EntityError::field(&self.id, entity, &fault, snippet).into());
                }
            }
            let dict = entity
                .fields(Entity::BASE)
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect();
            if let Some(fault) = Check::default().dict(&config.fields, &dict) {
                let snippet = entity.from().snippet(project, fault.steps());
                return Err(EntityError::field(&self.id, entity, &fault, snippet).into());
            }
        }
        Ok(())
    }

    /// Build the alias index, refusing a name that would reach two entities,
    /// an alias equal to another entity's own id included.
    ///
    /// A name that already reaches *this* entity is not a clash: an entity may
    /// spell one alias twice (`Bob` and `BOB` slug alike) or restate its own
    /// id, and neither leaves a reader anywhere ambiguous.
    fn index(&mut self, project: &Project) -> Result<()> {
        for entity in self.entities.values() {
            for alias in entity.aliases() {
                let clash = match self.entities.get(alias) {
                    Some(other) => Some(other.id()),
                    None => self.aliases.get(alias).map(String::as_str),
                };
                if let Some(first) = clash.filter(|first| *first != entity.id()) {
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

    pub fn slots(&self) -> &Slots {
        &self.slots
    }

    /// The named field set this registry took, if it took one.
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

    /// The page that declares the entity a term names, in `lang`; an entity
    /// written in a roster has none.
    pub fn page(&self, term: &str, lang: &str) -> Option<&std::path::Path> {
        self.get(term)?.page(lang)
    }

    /// The id `term` resolves to: its own, or the entity an alias reaches.
    /// Every consumer that *groups* by term has to call it first, or an alias
    /// splits one entity across two terms.
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

/// Every registry, in id order, as everything reading one sees it.
impl std::hash::Hash for Registries {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::hash::Hash for Registry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            id,
            shape,
            slots,
            unknown,
            entities,
            aliases,
        } = self;
        (id, shape, slots, unknown, entities, aliases).hash(state);
    }
}

impl std::hash::Hash for Entity {
    /// `from` is left out: where an entity was read is what a diagnostic
    /// points at, never what it resolves to.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            id,
            aliases,
            fields,
            from: _,
            editions,
            pages,
        } = self;
        (id, aliases, fields, editions, pages).hash(state);
    }
}

impl Registries {
    /// Build every registry from its sources, after discovery, so a `pages`
    /// source draws its entities from pages the plan has already read.
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
    pub fn none() -> &'static Self {
        static NONE: std::sync::LazyLock<Registries> =
            std::sync::LazyLock::new(Registries::default);
        &NONE
    }

    pub fn get(&self, id: &str) -> Option<&Registry> {
        self.0.get(id)
    }

    /// Resolve every reference every page writes, under each registry's own
    /// policy for a term nobody declared; under `unknown "synthesize"` the term
    /// is the entity, so nothing is reported.
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

    /// Whether a site declared any registry at all.
    pub fn any(&self) -> bool {
        !self.0.is_empty()
    }

    /// Every term of `page` that names an entity, the single reader of the
    /// taxonomy-to-registry wiring.
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
    pub taxonomy: &'a str,
    /// The frontmatter key it was written under, which is what locates it in
    /// the page; a taxonomy may read a key that is not its own id.
    pub key: &'a str,
    /// Which term of that key this is.
    pub index: usize,
    pub registry: &'a Registry,
    /// The term, as the page wrote it.
    pub term: &'a str,
    /// What the page claims about it, where the taxonomy says; a term under a
    /// taxonomy with no credit claims nothing.
    pub credit: Option<Credit>,
}

impl Reference<'_> {
    pub fn entity(&self) -> Option<&Entity> {
        self.registry.get(self.term)
    }

    /// This reference as the failure it is when nothing answers it, underlined
    /// at the term itself and at the severity the registry asked for.
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
