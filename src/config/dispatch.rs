//! Table-driven config dispatch.
//!
//! [`Block`] matches a scope's child nodes by name; [`Attrs`] matches a node's
//! `key=value` entries. Each dispatch table is the *single source of truth* for
//! that scope's valid keys: [`Keys`] derives "unknown key" errors (with a
//! nearest-match hint) from the very same table, so suggestions never drift from
//! what actually parses.
//!
//! A config struct carries its own table by implementing [`Section`] (a `{ .. }`
//! block) or [`Attributed`] (a `key=value` line), which is also where the merge
//! policy lives: sections fill in place, lists replace wholesale.

use itertools::Itertools;
use kdl::{KdlIdentifier, KdlNode, KdlValue};
use miette::SourceSpan;

use crate::error::{BaudelaireErrorKind, ConfigError, Result};
use crate::ui::{Code, markup};

use super::node::{EntryExt, NodeExt};

/// A `(key, kind, doc, handler)` rule for a node-keyed [`Block`] scope.
///
/// The first three columns are what [`Row`] carries into the generated
/// reference, and the fourth is what actually parses the key. They are one
/// tuple rather than a table beside the handlers so that documenting a key and
/// implementing it are the same edit: a new key cannot be added without a
/// description, and a removed one cannot linger in the docs.
type Rule<T> = (
    &'static str,
    Kind,
    &'static str,
    fn(&mut T, &KdlNode, &str) -> Result<()>,
);

/// A `(key, kind, doc, handler)` rule for an attribute-keyed [`Attrs`] scope.
type Attr<T> = (
    &'static str,
    Kind,
    &'static str,
    fn(&mut T, &KdlValue, &str, SourceSpan) -> Result<()>,
);

/// The shape of the value a key takes, for the generated reference.
///
/// Not derivable from the handler: a closure calling `n.string(t, 0)` is opaque,
/// so what a key accepts has to be declared alongside it.
#[derive(Clone, Copy)]
pub enum Kind {
    /// A single string: `site "My site"`.
    Text,
    /// A boolean: `prune #false`.
    Flag,
    /// A whole number: `port 3000`.
    Number,
    /// A byte size, with or without a unit: `html "50kB"`, `js 0`.
    Size,
    /// A filesystem path, relative to the project root: `content "content"`.
    Path,
    /// A URL: `url "https://example.com"`.
    Url,
    /// A permalink template: `permalink "/{slug}/"`.
    Template,
    /// One of a fixed set of names. Carried as a function over
    /// [`Named::names`](crate::config::Named::names) rather than as a literal
    /// list, so the names the reference prints are read out of the very table
    /// that parses them.
    Choice(Names),
    /// Any number of strings on one line: `footnotes "article" "main"`.
    Texts,
    /// Any number of whole numbers on one line: `widths 480 960 1440`.
    Numbers,
    /// A block of free `key=value` pairs, the keys chosen by the author.
    Table,
    /// A nested block, whose own keys are these.
    Block(Rows),
    /// A block of repeated child nodes, each named by the author and each
    /// accepting these keys.
    Items(Rows),
    /// A block of repeated child nodes, each named by the author and each
    /// accepting *any top-level key*.
    ///
    /// Its own variant rather than [`Kind::Items(Config::rows)`](Kind::Items), which is
    /// what it means: that spelling is honest and would send the reference
    /// walker into an infinite recursion, since a profile can hold a `profiles`
    /// block of its own.
    Overlay,
}

/// A scope's documented rows, as a function rather than a slice so a section can
/// name its children without this module knowing their Rust types, and so a
/// cyclic shape could not deadlock a `static`.
pub type Rows = fn() -> Vec<Row>;

/// The accepted spellings of a [`Kind::Choice`] key.
pub type Names = fn() -> Vec<&'static str>;

/// One key, as the reference renders it.
pub struct Row {
    pub key: &'static str,
    pub kind: Kind,
    pub doc: &'static str,
}

impl Row {
    /// The rows of a node-keyed table, the shape both [`Section`] and
    /// [`Attributed`] hand to the reference.
    fn of<F>(table: &'static [(&'static str, Kind, &'static str, F)]) -> Vec<Self> {
        table
            .iter()
            .map(|&(key, kind, doc, _)| Self { key, kind, doc })
            .collect()
    }
}

/// A node-keyed scope (child nodes matched by name), e.g. the top-level config
/// or a `serve { ... }` block. The rule table is the single source of truth for
/// valid keys.
pub(super) struct Block<T: 'static>(pub(super) &'static [Rule<T>]);

impl<T> Block<T> {
    /// Apply this scope's rules to every node in `nodes`, erroring on the first
    /// unrecognized key (with a nearest-match suggestion).
    pub(super) fn apply(&self, value: &mut T, nodes: &[KdlNode], text: &str) -> Result<()> {
        for node in nodes {
            let key = node.name().value();
            match self.0.iter().find(|(k, ..)| *k == key) {
                Some((.., handler)) => handler(value, node, text)?,
                None => return Err(Keys::unknown_key(self.0, text, key, NodeExt::span(node))),
            }
        }
        Ok(())
    }

    /// This scope's keys, as the reference renders them.
    fn rows(&self) -> Vec<Row> {
        Row::of(self.0)
    }
}

/// A config section: a struct filled from a node's `{ .. }` block, whose
/// [`RULES`](Section::RULES) table is the single source of truth for the keys
/// that block accepts. Every node-keyed scope in the config is one of these, so
/// the fill-in-place, presence-enables, and optional-backend policies are
/// written once here instead of once per section.
pub(super) trait Section: Sized + 'static {
    /// This section's `(key, kind, doc, handler)` table.
    const RULES: Block<Self>;

    /// This section's keys, as the reference renders them.
    ///
    /// A `fn() -> Vec<Row>` and not a constant, so a parent naming a child
    /// writes [`Kind::Block(Child::rows)`](Kind::Block) and never repeats the child's key
    /// list. That indirection is what makes the generated reference a walk of
    /// the same tables that parse, rather than a second description of them.
    fn rows() -> Vec<Row> {
        Self::RULES.rows()
    }

    /// Run before a block's keys are applied. A section that is turned on by the
    /// mere presence of its block flips its `enabled` flag here and returns
    /// `true`, so that rule lives with the section rather than at every parent
    /// mentioning it.
    ///
    /// The return value is what lets a *bare* node with no `{ }` mean "just turn
    /// it on": the docs promise that `generate { robots }` enables robots.txt by
    /// existing, and it used to be a hard `missing_children` error instead.
    /// Reporting it from the same override that does the enabling is what keeps
    /// the two from disagreeing.
    fn enable(&mut self) -> bool {
        false
    }

    /// Apply a node's `{ .. }` children onto `self`, *filling in place*: a key
    /// the block omits keeps the value it already had, which is what lets a
    /// profile override one key of a section and inherit its siblings.
    ///
    /// A node with no block at all is the "presence is the switch" spelling, and
    /// is accepted only where there is a switch to flip. For a section that
    /// merely holds settings, a bare `paths` configures nothing and is far more
    /// likely a forgotten block than an intent, so it still errors.
    fn fill(&mut self, node: &KdlNode, text: &str) -> Result<()> {
        let switch = self.enable();
        match node.children() {
            Some(block) => Self::RULES.apply(self, block.nodes(), text),
            None if switch => Ok(()),
            None => Err(ConfigError::missing_children(text, NodeExt::span(node)).into()),
        }
    }

    /// Apply a sequence of nodes onto `self`: the top-level document, or the
    /// single node of a profile overlaid on it.
    fn apply(&mut self, nodes: &[KdlNode], text: &str) -> Result<()> {
        Self::RULES.apply(self, nodes, text)
    }

    /// Fill a section that is absent until configured (a deploy or announce
    /// backend): the block's presence creates it, and an existing value is
    /// filled onto rather than replaced, so a profile tuning one key keeps the
    /// rest.
    fn optional(target: &mut Option<Self>, node: &KdlNode, text: &str) -> Result<()>
    where
        Self: Default,
    {
        let mut section = target.take().unwrap_or_default();
        section.fill(node, text)?;
        *target = Some(section);
        Ok(())
    }
}

/// A config item written as a single node carrying `key=value` attributes (a
/// collection, a taxonomy, an image format's tuning). The [`Attrs`] counterpart
/// of [`Section`].
pub(super) trait Attributed: Sized + 'static {
    /// This item's `(attribute, kind, doc, handler)` table.
    const ATTRS: Attrs<Self>;

    /// This item's attributes, as the reference renders them. The [`Section`]
    /// counterpart, for the same reason.
    fn rows() -> Vec<Row> {
        Self::ATTRS.rows()
    }

    /// How many leading positional arguments the caller consumes itself (a
    /// collection's glob); any other positional is an error.
    const LEADING: usize = 0;

    /// Apply the node's named attributes onto `self`.
    fn read(&mut self, node: &KdlNode, text: &str) -> Result<()> {
        Self::ATTRS.apply(self, node, text, Self::LEADING)
    }
}

/// An attribute-keyed scope (a node's `key=value` entries), e.g. a single
/// `content { taxonomies { tags listing=.. } }` line. Same single-source-of-truth
/// contract as [`Block`], but handlers receive the attribute value.
pub(super) struct Attrs<T: 'static>(pub(super) &'static [Attr<T>]);

impl<T> Attrs<T> {
    /// Apply named attributes of `node`, erroring on the first unrecognized
    /// attribute. At most `leading` positional (unnamed) entries are tolerated,
    /// and only at the front of the node (the caller consumes them, e.g. a
    /// collection's glob): any other positional would be silently discarded,
    /// so it errors instead.
    pub(super) fn apply(
        &self,
        value: &mut T,
        node: &KdlNode,
        text: &str,
        leading: usize,
    ) -> Result<()> {
        let span = NodeExt::span(node);
        for (position, entry) in node.entries().iter().enumerate() {
            let Some(key) = entry.name().map(KdlIdentifier::value) else {
                if position >= leading {
                    return Err(ConfigError::unexpected_argument(
                        text,
                        &entry.value().to_string(),
                        node.name().value(),
                        EntryExt::span(entry),
                    )
                    .into());
                }
                continue;
            };
            match self.0.iter().find(|(k, ..)| *k == key) {
                Some((.., handler)) => handler(value, entry.value(), text, span)?,
                None => return Err(Keys::unknown_key(self.0, text, key, span)),
            }
        }
        Ok(())
    }

    /// This scope's attributes, as the reference renders them.
    fn rows(&self) -> Vec<Row> {
        Row::of(self.0)
    }
}

/// The valid keys of a scope, derived from its dispatch table (never a separate
/// hand-kept list). Builds "unknown key" errors carrying a nearest-match hint.
pub(crate) struct Keys<'a>(pub(super) &'a [&'a str]);

impl<'a> Keys<'a> {
    /// The single "closest known name" helper, reused wherever a typo should
    /// suggest a valid name (config keys, frontmatter fields).
    pub(crate) fn of(names: &'a [&'a str]) -> Self {
        Self(names)
    }
}

impl Keys<'_> {
    /// Build an unknown-*key* error (a structural node/attribute name) from any
    /// dispatch `table`. The table is the sole source of truth for validity, so
    /// suggestions can never drift from what actually parses.
    pub(super) fn unknown_key<F>(
        table: &[(&'static str, Kind, &'static str, F)],
        text: &str,
        key: &str,
        span: SourceSpan,
    ) -> BaudelaireErrorKind {
        let names: Vec<&str> = table.iter().map(|(k, ..)| *k).collect();
        ConfigError::unknown_key(text, key, Keys(&names).help(key, "keys"), span).into()
    }

    /// Build an unknown-*value* error (an unrecognized enum variant supplied as
    /// a value) from an allowed-values `table`: the value counterpart to
    /// [`Keys::unknown_key`].
    pub(super) fn unknown_value<F>(
        table: &[(&'static str, F)],
        text: &str,
        value: &str,
        span: SourceSpan,
    ) -> BaudelaireErrorKind {
        let names: Vec<&str> = table.iter().map(|(k, _)| *k).collect();
        ConfigError::unknown_value(text, value, Keys(&names).help(value, "values"), span).into()
    }

    /// "did you mean ..? valid `noun`: .." help for an unrecognized name, reused
    /// wherever a name set drives validity (dispatch keys, profile names,
    /// virtual Typst modules).
    ///
    /// Laid out to be read rather than parsed: the suggestion, which is the
    /// answer in the common case, gets a line to itself, and each valid name a
    /// code span of its own. Two dozen bare words separated by commas are one
    /// wall of text, with the comma the only thing telling one name from the
    /// next. The break survives miette's wrapper, which re-indents the rest into
    /// the help column.
    pub(crate) fn help(&self, unknown: &str, noun: &str) -> String {
        let suggestion = match self.nearest(unknown) {
            Some(near) => markup!("did you mean `{}`?\n", near),
            None => String::new(),
        };
        let names = self.0.iter().map(Code).format(", ");
        format!("{suggestion}{}{names}", markup!("valid {}: ", noun))
    }

    /// The valid key within edit distance 2 of `unknown` (a typo), if any.
    pub(crate) fn nearest(&self, unknown: &str) -> Option<&str> {
        self.0
            .iter()
            .copied()
            .map(|candidate| (candidate, Self::distance(candidate, unknown)))
            .filter(|&(_, d)| d <= 2)
            .min_by_key(|&(_, d)| d)
            .map(|(candidate, _)| candidate)
    }

    /// Levenshtein edit distance between two words.
    fn distance(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        let mut curr = vec![0; b.len() + 1];
        for (i, &ca) in a.iter().enumerate() {
            curr[0] = i + 1;
            for (j, &cb) in b.iter().enumerate() {
                let cost = usize::from(ca != cb);
                curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        prev[b.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::Keys;

    #[test]
    fn suggests_the_nearest_key_for_a_typo() {
        assert_eq!(
            Keys(&["content", "dist"]).nearest("conten"),
            Some("content")
        );
        assert_eq!(Keys(&["port", "bind"]).nearest("prt"), Some("port"));
    }

    #[test]
    fn offers_no_suggestion_for_unrelated_words() {
        assert_eq!(Keys(&["content", "dist"]).nearest("xyzzy"), None);
    }

    #[test]
    fn help_lists_valid_keys_and_suggestion() {
        // The suggestion answers the common case, so it gets a line of its own,
        // and every valid name is a code span rather than a bare word in a
        // comma list.
        assert_eq!(
            Keys(&["pretty", "indent"]).help("pruty", "keys"),
            "did you mean `pretty`?\nvalid keys: `pretty`, `indent`"
        );
    }
}
