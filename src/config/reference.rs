//! The config schema, flattened for documentation.
//!
//! Walks the same [`Section`](super::dispatch::Section) tables that parse a
//! config, so the reference cannot describe a key that does not exist or miss
//! one that does. Nothing here knows what any key *means*: that text lives on
//! the rule beside its handler, which is what makes adding a key and
//! documenting it the same edit.
//!
//! One walk, two renderings: [`Module`] for the docs site and [`Terminal`] for
//! `baudelaire reference`. Both are Display adapters over the same [`Reference`]
//! rather than methods on it, so a third output is a third adapter and never a
//! second walk.

use std::fmt;

use super::Config;
use super::dispatch::Section;
use crate::codegen::{Typst, Value};

// Re-exported rather than left behind `pub(crate) mod dispatch`: `Kind` is part
// of what an [`Entry`] says, so a caller outside the crate (the reference test)
// has to be able to name it.
pub use super::dispatch::{Kind, Row, Rows};

/// Every key the config accepts, depth-first, in the order the tables declare
/// them.
pub struct Reference(Vec<Entry>);

/// One key, at its full path.
pub struct Entry {
    /// The dotted path, e.g. `assets.images.responsive.widths`.
    pub path: String,
    /// The key's own name, the last segment of the path.
    pub key: &'static str,
    pub kind: Kind,
    pub doc: &'static str,
    /// How many blocks deep, so a renderer can indent without re-parsing the
    /// path.
    pub depth: usize,
}

impl Reference {
    /// The whole schema.
    pub fn new() -> Self {
        let mut entries = Vec::new();
        Self::walk(Config::rows, "", 0, &mut entries);
        Self(entries)
    }

    /// The schema below a dotted path, the path's own key first: what
    /// `baudelaire reference assets.images` prints.
    ///
    /// `None` when nothing is named that, which the caller turns into an error
    /// listing what does exist; an empty `Reference` would read as "this key
    /// has no settings", which is a different and wrong answer.
    pub fn at(path: &str) -> Option<Self> {
        let all = Self::new();
        let start = all.0.iter().position(|e| e.path == path)?;
        let root = all.0[start].depth;
        let len = all.0[start + 1..]
            .iter()
            .take_while(|e| e.depth > root)
            .count();
        Some(Self(
            all.0
                .into_iter()
                .skip(start)
                .take(len + 1)
                .map(|e| Entry {
                    depth: e.depth - root,
                    ..e
                })
                .collect(),
        ))
    }

    /// Every key, in declaration order.
    pub fn entries(&self) -> &[Entry] {
        &self.0
    }

    /// Every dotted path, for the did-you-mean on an unknown one. Derived from
    /// the same walk that would have printed them, so a suggestion is always a
    /// path that works.
    pub fn paths(&self) -> Vec<&str> {
        self.0.iter().map(|e| e.path.as_str()).collect()
    }

    /// Append `rows`, then descend into whichever of them are blocks.
    ///
    /// [`Kind::Overlay`] is the one shape that does not recurse: a profile
    /// accepts every top-level key, so walking into it would be walking the
    /// whole document again, for ever.
    fn walk(rows: super::dispatch::Rows, prefix: &str, depth: usize, out: &mut Vec<Entry>) {
        for row in rows() {
            let path = match prefix.is_empty() {
                true => row.key.to_owned(),
                false => format!("{prefix}.{}", row.key),
            };
            let nested = match row.kind {
                Kind::Block(rows) | Kind::Items(rows) => Some(rows),
                _ => None,
            };
            out.push(Entry {
                path: path.clone(),
                key: row.key,
                kind: row.kind,
                doc: row.doc,
                depth,
            });
            if let Some(rows) = nested {
                Self::walk(rows, &path, depth + 1, out);
            }
        }
    }
}

impl Default for Reference {
    fn default() -> Self {
        Self::new()
    }
}

/// The reference as a terminal tree: what `baudelaire reference` writes.
///
/// Indented by nesting depth rather than printing each full dotted path, so the
/// shape of the config is legible down the left edge. Colour follows the same
/// palette as the rest of the CLI, and drops out on its own when stdout is not
/// a terminal.
pub struct Terminal<'a>(pub &'a Reference);

impl fmt::Display for Terminal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use owo_colors::{OwoColorize, Stream::Stdout};

        // The description column, measured over what will actually be printed
        // rather than fixed, so a narrow subtree is not indented to the width of
        // the whole document.
        let width = self
            .0
            .entries()
            .iter()
            .map(|e| e.depth * 2 + e.key.len() + e.kind.label().len() + 3)
            .max()
            .unwrap_or(0);
        for entry in self.0.entries() {
            let indent = " ".repeat(entry.depth * 2);
            let label = entry.kind.label();
            let plain = indent.len() + entry.key.len() + label.len() + 3;
            writeln!(
                f,
                "{indent}{}  {}{}{}",
                entry
                    .key
                    .if_supports_color(Stdout, |t| t.green().bold().to_string()),
                label.if_supports_color(Stdout, |t| t.dimmed().to_string()),
                " ".repeat(width.saturating_sub(plain) + 2),
                entry.doc,
            )?;
        }
        Ok(())
    }
}

/// The reference as a generated Typst data module, which the docs site's
/// reference page imports and renders.
///
/// Nothing here formats markup: the module is one binding of plain data, so how
/// the reference *reads* is a typst template in the docs site, in the docs'
/// own style, and this crate only has to keep saying what the keys are. It also
/// means no hand-rolled escaping, since every string goes out through
/// [`codegen::Value`].
pub struct Module<'a>(pub &'a Reference);

/// One entry as the record the template destructures.
impl From<&Entry> for Value {
    fn from(entry: &Entry) -> Self {
        Self::dict([
            ("path", Self::str(&entry.path)),
            ("key", Self::str(entry.key)),
            ("shape", Self::str(entry.kind.label())),
            ("doc", Self::str(entry.doc)),
            // The depth is how deep the dispatch tables nest, a handful at
            // most, so the conversion cannot fail on any real reference.
            (
                "depth",
                Self::Int(i64::try_from(entry.depth).expect("config nesting depth fits an i64")),
            ),
            ("section", Self::Bool(entry.kind.section())),
        ])
    }
}

impl fmt::Display for Module<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "// Generated by `just reference` from the dispatch tables in\n\
             // `src/config/parse.rs`. Do not edit: `cargo nextest run --test reference`\n\
             // fails when this file and those tables disagree.\n"
        )?;
        // One entry per line rather than one rendered array: the file is
        // committed, and a diff that names the key that changed is worth the
        // extra loop.
        writeln!(f, "#let entries = (")?;
        for entry in self.0.entries() {
            writeln!(f, "  {},", Typst(&Value::from(entry)))?;
        }
        writeln!(f, ")")
    }
}

impl Kind {
    /// Whether this key opens a block of its own keys, rather than holding a
    /// value: what a renderer turns into a heading instead of a table row.
    pub fn section(self) -> bool {
        matches!(self, Self::Block(_) | Self::Items(_) | Self::Overlay)
    }

    /// How this shape is named to a reader, in both renderings.
    ///
    /// A `String` and not a `&'static str` because a `Choice` spells out the
    /// names it accepts, read from the enum's own table.
    pub fn label(self) -> String {
        match self {
            Self::Text => "text".to_owned(),
            Self::Flag => "flag".to_owned(),
            Self::Number => "number".to_owned(),
            Self::Size => "size".to_owned(),
            Self::Path => "path".to_owned(),
            Self::Url => "url".to_owned(),
            Self::Template => "template".to_owned(),
            Self::Texts => "text ..".to_owned(),
            Self::Numbers => "number ..".to_owned(),
            Self::Table => "key=value ..".to_owned(),
            Self::Choice(names) => names().join(" | "),
            Self::Block(_) => "block".to_owned(),
            Self::Items(_) => "named blocks".to_owned(),
            Self::Overlay => "named overlays".to_owned(),
        }
    }
}
