//! The config spelling of an enum: one table per type, read both ways.

/// An enum spelled out in config as one of a fixed set of names.
///
/// [`Named::NAMES`] is that set, read both ways, so a variant can never parse
/// under one spelling and be generated under another.
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

    /// The variant a config name spells, or `None` when nothing spells it.
    fn of(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, variant)| *variant)
    }

    /// Every spelling this enum accepts, in declaration order.
    fn names() -> Vec<&'static str> {
        Self::NAMES.iter().map(|(name, _)| *name).collect()
    }
}
