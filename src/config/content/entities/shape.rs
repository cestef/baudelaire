//! `content { entities { <id> { shape } } }`: a field set and its slots, named.
//!
//! A shape is a row in the tables below, never a type in the code: nothing
//! downstream branches on which shape a registry named.

use super::slots::Slots;
use crate::config::{FieldSchema, FieldType, Named};

/// A named field set a registry can take instead of declaring its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    Person,
    /// A company, a publisher, a band.
    Organization,
}

impl Named for Shape {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("person", Self::Person),
        ("organization", Self::Organization),
    ];
}

/// One field a shape declares: its key, and a constructor for what it holds,
/// since [`FieldType`] owns the types it wraps and no constant can build one.
type Field = (&'static str, fn() -> FieldType);

/// The `person` fields, and the slots that read them.
const PERSON: &[Field] = &[
    ("name", || FieldType::Str),
    ("url", || FieldType::Str),
    ("avatar", || FieldType::Str),
    ("email", || FieldType::Str),
    // Unconstrained: the slot takes a list of URLs or a dictionary of platform
    // to URL, and typing it either way would refuse one of the two roster
    // dialects.
    ("socials", || FieldType::Any),
];

/// The `organization` fields.
const ORGANIZATION: &[Field] = &[
    ("name", || FieldType::Str),
    ("url", || FieldType::Str),
    ("logo", || FieldType::Str),
];

impl Shape {
    /// The fields this shape declares, every one of them optional: a shape
    /// types a field, it does not require one.
    pub fn fields(self) -> Vec<(String, FieldSchema)> {
        self.table()
            .iter()
            .map(|&(key, ty)| {
                (
                    key.to_owned(),
                    FieldSchema {
                        ty: ty(),
                        optional: true,
                        ..FieldSchema::default()
                    },
                )
            })
            .collect()
    }

    /// The slots this shape fills, which a registry's own `slots` line
    /// overrides one at a time.
    pub fn slots(self) -> Slots {
        let field = |name: &str| Some(name.to_owned());
        match self {
            Self::Person => Slots {
                display: field("name"),
                url: field("url"),
                image: field("avatar"),
                email: field("email"),
                same_as: field("socials"),
            },
            Self::Organization => Slots {
                display: field("name"),
                url: field("url"),
                image: field("logo"),
                email: None,
                same_as: None,
            },
        }
    }

    /// What schema.org calls this kind of thing, for the one vocabulary that
    /// types its objects.
    pub fn schema(self) -> &'static str {
        match self {
            Self::Person => "Person",
            Self::Organization => "Organization",
        }
    }

    fn table(self) -> &'static [Field] {
        match self {
            Self::Person => PERSON,
            Self::Organization => ORGANIZATION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Shape;
    use crate::config::Named;

    /// Every slot a shape fills has to name a field that shape declares, or the
    /// registry it seeds fails its own check.
    #[test]
    fn every_shape_fills_slots_from_its_own_fields() {
        for (name, shape) in Shape::NAMES {
            let fields = shape.fields();
            for (slot, field) in shape.slots().filled() {
                assert!(
                    fields.iter().any(|(key, _)| key == field),
                    "the `{name}` shape fills `{slot}` from `{field}`, which it does not declare"
                );
            }
        }
    }
}
