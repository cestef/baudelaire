//! The config spelling of an enum: one table per type, read both ways.

/// An enum spelled out in config as one of a fixed set of names.
///
/// [`Named::NAMES`] is that set: config parsing maps through it, its
/// unknown-value suggestions are derived from it, and [`Named::name`] reads
/// back out of it. One table, so a variant can never parse under one spelling
/// and be generated under another.
pub trait Named: Copy + PartialEq + Sized + 'static {
    const NAMES: &'static [(&'static str, Self)];

    /// The name this variant is configured as, and the one generated code sees.
    fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(_, variant)| *variant == self)
            .map(|(name, _)| *name)
            .expect("NAMES lists every variant")
    }

    /// The variant a config name spells, if any: the read direction of
    /// [`NAMES`](Named::NAMES), and the only way a name becomes a variant.
    fn of(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, variant)| *variant)
    }

    /// Every spelling this enum accepts, in declaration order: what the
    /// generated reference lists for a `Kind::Choice` key, read out of the same
    /// table that parses them.
    fn names() -> Vec<&'static str> {
        Self::NAMES.iter().map(|(name, _)| *name).collect()
    }
}
