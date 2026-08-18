//! Where a key is set, read back out of the authored config.
//!
//! The dispatch tables are the guide: at every node, a name they know is a key
//! and a name they do not is the author's own (a collection's id, a redirect's
//! path), which is what lets one dotted path find a key nested under names this
//! crate never chose.

use kdl::{KdlDocument, KdlNode};

use super::Config;
use super::reference::Reference;
use crate::error::cli::UnknownKey;
use crate::error::{ConfigError, Result};

/// One place a key is written.
pub struct Sighting {
    /// The profile that set it, or `None` for the config's own body.
    pub profile: Option<String>,
    /// One-based, as an editor counts.
    pub line: usize,
    /// The author's own names on the way to it, outermost first: the collection
    /// a `permalink` belongs to, the language a `strings` block is under.
    pub named: Vec<String>,
    /// The node's own line, as it was written.
    pub written: String,
    /// What it was set to, as written: the node's arguments, or the whole block
    /// for a key that opens one.
    pub value: String,
    /// The first argument as a script wants it: a string without the quotes it
    /// was written with, anything else as written.
    pub scalar: String,
    /// The node and everything under it, as written.
    pub block: String,
}

/// Every place one key is written in one config text.
pub struct Sightings(Vec<Sighting>);

impl Sightings {
    /// Where `key` is set in `text`, the config's own body first and each
    /// profile after it, in the order they are declared.
    ///
    /// The key must be one the dispatch tables know; anything else is a
    /// [`ConfigError::unknown_key`] rather than an empty answer.
    pub fn of(config: &Config, key: &str) -> Result<Self> {
        if Reference::at(key).is_none() {
            return Err(UnknownKey::at(key).into());
        }
        let text = config.text();
        let doc: KdlDocument = text.parse().map_err(|e| ConfigError::parse(text, e))?;
        let lines = Lines::of(text);
        let mut out = Vec::new();
        let walk = |nodes: &[KdlNode], profile: Option<String>, out: &mut Vec<Sighting>| {
            Walk {
                key,
                lines: &lines,
                profile,
            }
            .nodes(nodes, "", &mut Vec::new(), out);
        };
        walk(doc.nodes(), None, &mut out);
        for (name, profile) in &config.profiles {
            walk(profile.nodes(), Some(name.clone()), &mut out);
        }
        Ok(Self(out))
    }

    pub fn all(&self) -> &[Sighting] {
        &self.0
    }
}

/// One pass over one document, looking for a single key.
struct Walk<'a> {
    key: &'a str,
    lines: &'a Lines<'a>,
    profile: Option<String>,
}

impl Walk<'_> {
    fn nodes(
        &self,
        nodes: &[KdlNode],
        path: &str,
        named: &mut Vec<String>,
        out: &mut Vec<Sighting>,
    ) {
        for node in nodes {
            self.node(node, path, named, out);
        }
    }

    /// Follow one node: a name the tables know continues the dotted path, and
    /// one they do not is the author's, which leaves the path where it was.
    fn node(&self, node: &KdlNode, path: &str, named: &mut Vec<String>, out: &mut Vec<Sighting>) {
        let name = node.name().value();
        let below = if path.is_empty() {
            name.to_owned()
        } else {
            format!("{path}.{name}")
        };
        let known = Reference::at(&below).is_some();
        if known && below == self.key {
            out.push(self.sighting(node, named));
        }
        let Some(children) = node.children() else {
            return;
        };
        if known {
            self.nodes(children.nodes(), &below, named, out);
        } else {
            named.push(name.to_owned());
            self.nodes(children.nodes(), path, named, out);
            named.pop();
        }
    }

    fn sighting(&self, node: &KdlNode, named: &[String]) -> Sighting {
        let at = node.span().offset();
        Sighting {
            profile: self.profile.clone(),
            line: self.lines.at(at),
            named: named.to_vec(),
            written: self.lines.text(at).to_owned(),
            value: Written(node).to_string(),
            scalar: node
                .entries()
                .first()
                .map_or_else(String::new, |entry| match entry.value() {
                    kdl::KdlValue::String(text) => text.clone(),
                    other => other.to_string(),
                }),
            block: node.to_string().trim().to_owned(),
        }
    }
}

/// What a node was set to: its arguments and attributes as written, which is
/// the answer for every key that holds a value rather than a block.
struct Written<'a>(&'a KdlNode);

impl std::fmt::Display for Written<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let written: Vec<String> = self
            .0
            .entries()
            .iter()
            .map(|entry| entry.to_string().trim().to_owned())
            .collect();
        f.write_str(&written.join(" "))
    }
}

/// A text's line breaks, for turning an offset into what an editor would call
/// it and into the line it falls on.
struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn of(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        Self { text, starts }
    }

    /// The one-based line `at` falls on.
    fn at(&self, at: usize) -> usize {
        self.starts.partition_point(|start| *start <= at)
    }

    /// The line `at` falls on, trimmed.
    fn text(&self, at: usize) -> &str {
        let start = self.starts[self.at(at).saturating_sub(1)];
        self.text[start..]
            .split('\n')
            .next()
            .unwrap_or_default()
            .trim()
    }
}
