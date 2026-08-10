//! `content { entities { } }`: the registries a page's references resolve into.
//!
//! An entity is an id and a bag of typed fields, and a registry is a named set
//! of them: `people`, `organizations`, `series`. Nothing here is about any one
//! of those. A registry declares what its entities carry ([`shape`] or its own
//! `fields { }`), which field answers each question a renderer asks
//! ([`slots`]), and where they come from ([`source`]).
//!
//! What points *at* a registry is a taxonomy, through
//! [`TaxonomyConfig::entities`](crate::config::TaxonomyConfig): the terms a page
//! writes under `authors` are ids in the `people` registry. So a site gets term
//! pages, per-term feeds and listings from machinery that already exists, and
//! this block only has to answer "who is `zoe`".

pub mod shape;
pub mod slots;
pub mod source;

use kdl::KdlNode;

use crate::config::dispatch::Kind::{Block as Nested, Choice, Items, Line};
use crate::config::dispatch::{Attributed, Block, Keys, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{FieldSchema, Named};
use crate::error::{ConfigError, ConfigErrorKind, Result};

pub use shape::Shape;
pub use slots::Slots;
pub use source::{Declared, SourceConfig, SourcesConfig};

/// One registry: what its entities carry, where they come from, and what a
/// reference to one nobody declared means.
#[derive(Debug, Clone, Default, Hash)]
pub struct RegistryConfig {
    /// The named field set this registry took, if it took one.
    pub shape: Option<Shape>,
    /// What every entity carries, in declaration order: the `shape`'s fields
    /// with the registry's own filled over them. Empty constrains nothing.
    pub fields: Vec<(String, FieldSchema)>,
    /// Which field answers each question a renderer asks.
    pub slots: Slots,
    /// Where the entities come from, in declaration order.
    pub sources: Vec<SourceConfig>,
    /// What a reference nobody declared means. `None` until the site says,
    /// which is what lets the default depend on whether there is a roster to
    /// be missing from: see [`RegistryConfig::unknown`].
    unknown: Option<Unknown>,
}

/// What a reference to an entity the registry does not hold means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unknown {
    /// Fail the build, naming the page and suggesting a near id.
    Error,
    /// Report it and carry on, rendering the reference as its own text.
    Warn,
    /// Take the reference at face value: the id is the display name, and there
    /// is nothing else to know. What a site with no roster at all means, and
    /// what `author "Camille"` has always meant.
    Synthesize,
}

impl Named for Unknown {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("error", Self::Error),
        ("warn", Self::Warn),
        ("synthesize", Self::Synthesize),
    ];
}

impl RegistryConfig {
    /// What a reference nobody declared means here.
    ///
    /// A registry with a source defaults to refusing one: it is a roster of
    /// who exists, and naming somebody who is not in it is the typo the roster
    /// exists to catch. A registry with no source at all knows nobody, so every
    /// reference is taken at face value, which is what a site that never
    /// declared a registry has always done with `author "Camille"`.
    pub fn unknown(&self) -> Unknown {
        let default = match self.sources.is_empty() {
            true => Unknown::Synthesize,
            false => Unknown::Error,
        };
        self.unknown.unwrap_or(default)
    }

    /// One `people { .. }` block.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let id = node.name().value().to_owned();
        let mut registry = Self::default();
        registry.fill(node, text)?;
        registry.seed();
        registry.check(&id, node, text)?;
        Ok((id, registry))
    }

    /// Take from the named shape whatever the registry did not spell out.
    ///
    /// Fields fill in place, key by key, exactly as a config section does: a
    /// field the registry declares replaces the shape's of that name, and one
    /// the shape never had is added. So `shape "person"` with
    /// `fields { name "str" }` is person, with the name required, and the
    /// avatar and the socials still typed. Replacing the set wholesale would
    /// make naming both keys a contradiction, since a shape *is* a field set.
    ///
    /// Slots fill the same way, one at a time, which is what lets a registry
    /// spell one of them differently and inherit the rest.
    fn seed(&mut self) {
        let Some(shape) = self.shape else {
            return;
        };
        let mut fields = shape.fields();
        for (key, declared) in std::mem::take(&mut self.fields) {
            match fields.iter_mut().find(|(name, _)| *name == key) {
                Some(field) => field.1 = declared,
                None => fields.push((key, declared)),
            }
        }
        self.fields = fields;
        self.slots.under(&shape.slots());
    }

    /// Refuse a slot naming a field the registry does not declare.
    ///
    /// Checked here rather than where a renderer reads the slot, because there
    /// the answer is simply "no value": a `slots image="protrait"` would render
    /// every entity without its picture, out of a green build, and nothing
    /// would ever say the word.
    ///
    /// A registry declaring no fields at all constrains nothing, so its slots
    /// name whatever its sources happen to carry.
    fn check(&self, id: &str, node: &KdlNode, text: &str) -> Result<()> {
        if self.fields.is_empty() {
            return Ok(());
        }
        let declared: Vec<&str> = self.fields.iter().map(|(key, _)| key.as_str()).collect();
        for (slot, field) in self.slots.filled() {
            if declared.contains(&field) {
                continue;
            }
            return Err(ConfigError::at(
                text,
                ConfigErrorKind::EntitySlot {
                    slot,
                    registry: id.to_owned(),
                    field: field.to_owned(),
                    help: Keys::of(&declared).help(field, "fields"),
                },
                NodeExt::span(node),
            )
            .into());
        }
        Ok(())
    }
}

/// One registry's block: what its entities carry, and where they come from.
impl Section for RegistryConfig {
    const RULES: Block<Self> = Block(&[
        (
            "shape",
            Choice(Shape::names),
            "A named field set to take instead of declaring one.",
            |c, n, t| {
                c.shape = Some(n.arg(t, 0)?.one::<Shape>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
        (
            "fields",
            Items(FieldSchema::rows),
            "What every entity carries, one line per field. Declaring a field requires it; with a `shape`, a field of the same name replaces that shape's and any other is added.",
            |c, n, t| {
                c.fields = n.unique(t, "field", FieldSchema::item)?;
                Ok(())
            },
        ),
        (
            "slots",
            Line(Slots::rows),
            "Which field answers each question a renderer asks of an entity.",
            |c, n, t| c.slots.read(n, t),
        ),
        (
            "sources",
            Nested(SourcesConfig::rows),
            "Where the entities come from, read in the order written.",
            |c, n, t| {
                let mut sources = SourcesConfig::default();
                sources.fill(n, t)?;
                c.sources = sources.0;
                Ok(())
            },
        ),
        (
            "unknown",
            Choice(Unknown::names),
            "What a reference to an entity nobody declared means. Defaults to `error` where there is a roster, `synthesize` where there is not.",
            |c, n, t| {
                c.unknown = Some(n.arg(t, 0)?.one::<Unknown>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
    ]);
}
