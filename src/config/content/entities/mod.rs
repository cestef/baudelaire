//! `content { entities { } }`: the registries a page's references resolve into.
//!
//! An entity is an id and typed fields; a registry declares what it carries
//! ([`shape`] or `fields`), which field answers each renderer question
//! ([`slots`]), and where entities come from ([`source`]).

pub mod shape;
pub mod slots;
pub mod source;

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::Value;
use crate::config::dispatch::Kind::{Choice, Items, Line};
use crate::config::dispatch::{Attributed, Block, Keys, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::config::{FieldSchema, Named};
use crate::error::{ConfigError, ConfigErrorKind, Result};

pub use shape::Shape;
pub use slots::Slots;
pub use source::{Declared, SourceConfig, SourcesConfig};

/// One registry: what its entities carry, where they come from, and what an
/// undeclared reference means.
#[derive(Debug, Clone, Default, Hash, Table)]
pub struct RegistryConfig {
    /// A named field set to take instead of declaring one.
    #[key(opt choice(Shape))]
    pub shape: Option<Shape>,

    /// What every entity carries, one line per field. Declaring a field requires it; with a `shape`, a field of the same name replaces that shape's and any other is added.
    ///
    /// In declaration order: the `shape`'s fields with the registry's own
    /// filled over them. Empty constrains nothing.
    #[key(custom(
        Items(FieldSchema::rows),
        |c: &Self| Value::each(&c.fields, Attributed::values),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.fields = n.unique(t, "field", FieldSchema::item)?;
            Ok(())
        },
    ))]
    pub fields: Vec<(String, FieldSchema)>,

    /// Which field answers each question a renderer asks of an entity.
    #[key(custom(
        Line(Slots::rows),
        |c: &Self| c.slots.values(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| c.slots.read(n, t),
    ))]
    pub slots: Slots,

    /// Where the entities come from, read in the order written.
    ///
    /// In declaration order.
    #[key(custom(
        crate::config::dispatch::Kind::Block(SourcesConfig::rows),
        |c: &Self| SourcesConfig(c.sources.clone()).values(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let mut sources = SourcesConfig::default();
            sources.fill(n, t)?;
            c.sources = sources.0;
            Ok(())
        },
    ))]
    pub sources: Vec<SourceConfig>,

    /// What a reference to an entity nobody declared means. Defaults to `error` where there is a roster, `synthesize` where there is not.
    ///
    /// `None` until the site says; see [`RegistryConfig::unknown`] for the
    /// default.
    #[key(custom(
        Choice(Unknown::names),
        |c: &Self| Value::named(c.unknown()),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.unknown = Some(crate::config::value::ValueExt::one::<Unknown>(
                n.arg(t, 0)?,
                t,
                NodeExt::span(n),
            )?);
            Ok(())
        },
    ))]
    unknown: Option<Unknown>,
}

/// What a reference to an entity the registry does not hold means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unknown {
    /// Fail the build, naming the page and suggesting a near id.
    Error,
    /// Report it and carry on, rendering the reference as its own text.
    Warn,
    /// Take the reference at face value: the id is the display name.
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
    /// What a reference nobody declared means here: refused when there is a
    /// source to be missing from, synthesized when there is none.
    pub fn unknown(&self) -> Unknown {
        let default = if self.sources.is_empty() {
            Unknown::Synthesize
        } else {
            Unknown::Error
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

    /// Take from the named shape whatever the registry did not spell out,
    /// field by field rather than replacing the set.
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

    /// Refuse a slot naming a field the registry does not declare. Checked
    /// here, since to a renderer a missing field is silently "no value".
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
