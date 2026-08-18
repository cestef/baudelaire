//! Writing one key back into the authored config.
//!
//! KDL is edited as a document rather than as text, so everything the author
//! wrote around the key -- comments, blank lines, the order of blocks -- comes
//! back out unchanged.

use kdl::{KdlDocument, KdlEntry, KdlEntryFormat, KdlNode, KdlValue};

use super::Config;
use super::dispatch::Kind;
use super::key::Key;
use crate::error::cli::UnknownKey;
use crate::error::{ConfigError, Result};

/// One key set to one value, as the config text it produces.
pub struct Edit {
    key: String,
    entry: KdlEntry,
}

impl Edit {
    /// The edit setting `key` to `raw`, read as the shape the dispatch tables
    /// give that key: a `number` key takes a number, a `flag` a boolean, and
    /// everything else the string it was written as.
    ///
    /// A segment the tables do not know is the author's own name, which is what
    /// lets `content.collections.blog.sort` name a collection nobody but the
    /// author called `blog`.
    pub fn new(key: &str, raw: &str) -> Result<Self> {
        let kind = Key::new(key).kind().ok_or_else(|| UnknownKey::at(key))?;
        Ok(Self {
            key: key.to_owned(),
            entry: Value(kind).of(raw),
        })
    }

    /// `text` with the key set, or the error a build would have raised on the
    /// result: nothing is written that would not parse.
    pub fn applied(&self, text: &str) -> Result<String> {
        let mut doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        let path = Key::new(&self.key);
        Self::place(&mut doc, &path.segments(), self.entry.clone());
        let written = doc.to_string();
        Config::parse(&written)?;
        Ok(written)
    }

    /// Set the value on the node `segments` names, opening whatever blocks it
    /// takes to get there.
    fn place(doc: &mut KdlDocument, segments: &[&str], entry: KdlEntry) {
        let Some((name, rest)) = segments.split_first() else {
            return;
        };
        if doc.get(name).is_none() {
            doc.nodes_mut().push(KdlNode::new(*name));
        }
        let node = doc.get_mut(name).expect("the node was just placed");
        if rest.is_empty() {
            node.entries_mut().clear();
            node.push(entry);
            return;
        }
        if node.children().is_none() {
            node.ensure_children();
        }
        Self::place(
            node.children_mut().as_mut().expect("children were ensured"),
            rest,
            entry,
        );
    }
}

/// A raw command-line word as the KDL entry its key's shape asks for. A string
/// is written quoted, which bare KDL does not require and every config here
/// does.
struct Value(Kind);

impl Value {
    fn of(&self, raw: &str) -> KdlEntry {
        let value = match self.0 {
            Kind::Number => raw
                .parse::<i128>()
                .map_or_else(|_| KdlValue::String(raw.to_owned()), KdlValue::Integer),
            Kind::Flag => raw
                .parse::<bool>()
                .map_or_else(|_| KdlValue::String(raw.to_owned()), KdlValue::Bool),
            _ => KdlValue::String(raw.to_owned()),
        };
        let mut entry = KdlEntry::new(value.clone());
        if let KdlValue::String(written) = &value {
            entry.set_format(KdlEntryFormat {
                value_repr: format!("{written:?}"),
                leading: " ".to_owned(),
                ..KdlEntryFormat::default()
            });
        }
        entry
    }
}

#[cfg(test)]
mod tests {
    use super::Edit;

    const BASE: &str = "site \"T\"\n\n// kept\npaths {\n  dist \"public\"\n}\n";

    #[test]
    fn setting_a_key_leaves_everything_around_it_alone() {
        let out = Edit::new("paths.dist", "out")
            .expect("a key")
            .applied(BASE)
            .expect("valid");
        assert!(out.contains("dist \"out\""), "{out}");
        assert!(out.contains("// kept"), "the comment survived: {out}");
    }

    #[test]
    fn a_key_nothing_has_set_yet_opens_the_blocks_it_needs() {
        let out = Edit::new("generate.cards.width", "800")
            .expect("a key")
            .applied(BASE)
            .expect("valid");
        assert!(out.contains("generate"), "{out}");
        assert!(out.contains("width 800"), "a number, not a string: {out}");
    }

    #[test]
    fn a_flag_is_written_as_one() {
        let out = Edit::new("content.future", "true")
            .expect("a key")
            .applied(BASE)
            .expect("valid");
        assert!(out.contains("future #true"), "{out}");
    }

    #[test]
    fn a_value_the_key_refuses_is_never_written() {
        let edit = Edit::new("serve.port", "99999").expect("a key");
        assert!(edit.applied(BASE).is_err(), "a port out of range");
    }

    #[test]
    fn a_key_the_tables_do_not_have_is_refused_before_anything_is_written() {
        assert!(Edit::new("paths.dsit", "out").is_err());
    }
}
