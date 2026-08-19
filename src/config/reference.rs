//! The config schema, flattened for documentation.
//!
//! Walks the same [`Section`](super::dispatch::Section) tables that parse a
//! config, so the reference cannot describe a key that does not exist. One
//! walk, two Display adapters: [`Module`] for the docs site and [`Terminal`]
//! for `baudelaire reference`.

use std::fmt;

use super::Config;
use super::dispatch::Section;
use crate::codegen::{Typst, Value};

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
    /// How many blocks deep the key sits.
    pub depth: usize,
}

impl Reference {
    pub fn new() -> Self {
        let mut entries = Vec::new();
        Self::walk(Config::rows, "", 0, &mut entries);
        Self(entries)
    }

    /// The schema below a dotted path, the path's own key first.
    ///
    /// `None` when nothing is named that, which is a different answer from an
    /// empty `Reference` (a key with no settings of its own).
    pub fn at(path: &str) -> Option<Self> {
        let all = Self::whole();
        let start = all.0.iter().position(|e| e.path == path)?;
        let root = all.0[start].depth;
        let len = all.0[start + 1..]
            .iter()
            .take_while(|e| e.depth > root)
            .count();
        Some(Self(
            all.0
                .iter()
                .skip(start)
                .take(len + 1)
                .map(|e| Entry {
                    path: e.path.clone(),
                    depth: e.depth - root,
                    ..*e
                })
                .collect(),
        ))
    }

    /// The whole schema, walked once: `at` is called per dotted segment and per
    /// config node visited, and each walk allocates a `String` per key.
    fn whole() -> &'static Self {
        static WHOLE: std::sync::LazyLock<Reference> = std::sync::LazyLock::new(Reference::new);
        &WHOLE
    }

    pub fn entries(&self) -> &[Entry] {
        &self.0
    }

    /// Every dotted path, for the did-you-mean on an unknown one.
    pub fn paths(&self) -> Vec<&str> {
        self.0.iter().map(|e| e.path.as_str()).collect()
    }

    /// Append `rows`, then descend into whichever of them are blocks.
    ///
    /// [`Kind::Overlay`] does not recurse: a profile accepts every top-level
    /// key, so walking into it would never terminate.
    fn walk(rows: super::dispatch::Rows, prefix: &str, depth: usize, out: &mut Vec<Entry>) {
        for row in rows() {
            let path = if prefix.is_empty() {
                row.key.to_owned()
            } else {
                format!("{prefix}.{}", row.key)
            };
            let nested = match row.kind {
                Kind::Block(rows) | Kind::Items(rows) | Kind::Line(rows) | Kind::Lines(rows) => {
                    Some(rows)
                }
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

/// The reference as a terminal tree: what `baudelaire reference` writes,
/// indented by nesting depth rather than by full dotted path.
pub struct Terminal<'a>(pub &'a Reference);

impl fmt::Display for Terminal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use owo_colors::{OwoColorize, Stream::Stdout};

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
/// One binding of plain data and no markup: how the reference *reads* is a
/// template in the docs site.
pub struct Module<'a>(pub &'a Reference);

impl From<&Entry> for Value {
    fn from(entry: &Entry) -> Self {
        Self::dict([
            ("path", Self::str(&entry.path)),
            ("key", Self::str(entry.key)),
            ("shape", Self::str(entry.kind.label())),
            ("doc", Self::str(entry.doc)),
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
             // `src/config/`, where each block's module carries its own. Do not\n\
             // edit: `cargo nextest run --test reference` fails when this file\n\
             // and those tables disagree.\n"
        )?;
        writeln!(f, "#let entries = (")?;
        for entry in self.0.entries() {
            writeln!(f, "  {},", Typst(&Value::from(entry)))?;
        }
        writeln!(f, ")")
    }
}

impl Kind {
    /// Whether this key opens a block of its own keys rather than holding a
    /// value.
    pub fn section(self) -> bool {
        matches!(
            self,
            Self::Block(_)
                | Self::Items(_)
                | Self::Line(_)
                | Self::Lines(_)
                | Self::Tables
                | Self::Overlay
        )
    }

    /// How this shape is named to a reader, in both renderings. A `String`
    /// because a `Choice` spells out the names it accepts.
    pub fn label(self) -> String {
        match self {
            Self::Text => "text".to_owned(),
            Self::Flag => "flag".to_owned(),
            Self::Number => "number".to_owned(),
            Self::Size => "size".to_owned(),
            Self::Time => "duration".to_owned(),
            Self::Version => "version".to_owned(),
            Self::Level(names) => format!("flag | {}", names().join(" | ")),
            Self::Path => "path".to_owned(),
            Self::Asset => "asset path".to_owned(),
            Self::Url => "url".to_owned(),
            Self::Template => "template".to_owned(),
            Self::Texts => "text ..".to_owned(),
            Self::Numbers => "number ..".to_owned(),
            Self::Table => "key value ..".to_owned(),
            Self::Choice(names) => names().join(" | "),
            Self::Choices(names) => format!("({}) ..", names().join(" | ")),
            Self::Toggled(names, on) => {
                let on = on();
                let names: Vec<String> = names()
                    .into_iter()
                    .map(|name| {
                        if on.contains(&name) {
                            format!("{name}*")
                        } else {
                            name.to_owned()
                        }
                    })
                    .collect();
                format!("[-]({}) ..", names.join(" | "))
            }
            Self::Toggles => "[-]text ..".to_owned(),
            Self::Block(_) => "block".to_owned(),
            Self::Items(_) => "named blocks".to_owned(),
            Self::Tables => "named key value blocks".to_owned(),
            Self::Line(_) => "key=value ..".to_owned(),
            Self::Lines(_) => "named lines".to_owned(),
            Self::Overlay => "named overlays".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, Reference};

    #[test]
    fn a_multi_value_choice_renders_as_a_list() {
        let reference = Reference::new();
        for path in ["generate.feed.formats", "artifacts.bundles.formats"] {
            let entry = reference
                .entries()
                .iter()
                .find(|e| e.path == path)
                .unwrap_or_else(|| panic!("{path} is a key"));
            assert!(matches!(entry.kind, Kind::Choices(_)), "{path}");
            let label = entry.kind.label();
            assert!(label.ends_with(" .."), "{path}: {label}");
            assert!(label.contains(" | "), "{path}: {label}");
        }
    }

    #[test]
    fn a_single_choice_carries_no_list_mark() {
        let reference = Reference::new();
        let entry = reference
            .entries()
            .iter()
            .find(|e| e.path == "links.style")
            .expect("links.style is a key");
        let label = entry.kind.label();
        assert!(!label.ends_with(".."), "{label}");
        assert!(label.contains("clean"), "{label}");
    }
}
