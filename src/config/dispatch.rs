//! Table-driven config dispatch.
//!
//! [`Block`] matches a scope's child nodes by name; [`Attrs`] matches a node's
//! `key=value` entries. Each dispatch table is the single source of truth for
//! that scope's valid keys, [`Keys`] deriving "unknown key" errors from it.
//!
//! A config struct carries its own table by implementing [`Section`] (a `{ .. }`
//! block) or [`Attributed`] (a `key=value` line), which is also where the merge
//! policy lives: sections fill in place, lists replace wholesale.

use std::cell::Cell;

use itertools::Itertools;
use kdl::{KdlIdentifier, KdlNode, KdlValue};
use miette::SourceSpan;

use crate::error::{BaudelaireErrorKind, ConfigError, Result};
use crate::ui::{Code, markup};

use super::node::{EntryExt, NodeExt};
use super::value::Kdl;
use super::values::Value;

thread_local! {
    /// Whether this thread is inside an [`Overlaying`] pass.
    static OVERLAYING: Cell<bool> = const { Cell::new(false) };
}

/// A pass that lays one layer's nodes over a config already read: for as long as
/// the guard is alive, naming a section does not turn it on, since the switch is
/// a value the layer below decided and a section fills in place.
///
/// A layer that means to turn a section on says so on its line (`check #true`),
/// or names it with no block at all.
pub(super) struct Overlaying(bool);

impl Overlaying {
    /// Begin an overlay on the current thread; the previous mode is restored
    /// when the guard drops, so the guard has to be bound.
    #[must_use]
    pub(super) fn begin() -> Self {
        Self(OVERLAYING.replace(true))
    }

    fn active() -> bool {
        OVERLAYING.get()
    }
}

impl Drop for Overlaying {
    fn drop(&mut self) {
        OVERLAYING.set(self.0);
    }
}

/// A `(key, kind, doc, read, write)` rule for a node-keyed [`Block`] scope, in
/// one tuple so that documenting a key, reading it and writing it are the same
/// edit.
type Rule<T> = (
    &'static str,
    Kind,
    &'static str,
    Read<T>,
    fn(&mut T, &KdlNode, &str) -> Result<()>,
);

/// How a key is read back off the struct it was parsed into: the half of a row
/// that answers what a config *holds*, defaults and all.
pub(super) type Read<T> = fn(&T) -> Value;

/// The flag a switchable [`Section`] turns on by its own presence, read and
/// written in one declaration: see [`Section::SWITCH`].
pub(super) struct Switch<T> {
    pub(super) set: fn(&mut T, bool),
    pub(super) on: fn(&T) -> bool,
}

/// A `(key, kind, doc, read, write)` rule for an attribute-keyed [`Attrs`]
/// scope.
type Attr<T> = (
    &'static str,
    Kind,
    &'static str,
    Read<T>,
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
    /// A length of time, with or without a unit: `fresh "7d"`, `timeout 30`.
    Time,
    /// A browser version, `major[.minor[.patch]]`: `safari "15.4"`.
    Version,
    /// One of a fixed set of names, or the boolean that stands for "on at the
    /// default": `alt "warn"`, `alt #false`.
    Level(Names),
    /// A filesystem path, relative to the project root: `content "content"`.
    Path,
    /// A path a generated asset is served from, relative to the asset root:
    /// `path "css/utilities.css"`.
    ///
    /// Not [`Path`](Kind::Path): the same string names where the file is written
    /// and how a page links it, so it may neither be absolute nor escape.
    Asset,
    /// A URL: `url "https://example.com"`.
    Url,
    /// A permalink template: `permalink "/{slug}/"`.
    Template,
    /// One of a fixed set of names: `html "drop"`. Carried as a function over
    /// [`Named::names`](crate::config::Named::names), so the names the reference
    /// prints are read out of the table that parses them.
    Choice(Names),
    /// Any number of names from a fixed set, written on one line and replacing
    /// whatever the key held: `formats "rss" "atom"`.
    Choices(Names),
    /// Any number of names from a fixed set, where `-name` removes one from the
    /// key's defaults: `extensions "math" "-tables"`.
    ///
    /// Distinct from [`Choices`](Kind::Choices) in what a list means: there it
    /// replaces, here it amends. The second function names the ones that are on
    /// without being asked for.
    Toggled(Names, Names),
    /// The same `-name` grammar over an *open* set, where the names are not a
    /// table this crate owns: `typst { features "bundle" "-a11y-extras" }`.
    Toggles,
    /// Any number of strings on one line: `footnotes "article" "main"`.
    Texts,
    /// Any number of whole numbers on one line: `widths 480 960 1440`.
    Numbers,
    /// A block of free entries, the keys chosen by the author, one node each:
    /// `strings { next "Next"; prev "Previous" }`.
    ///
    /// Read by [`NodeExt::pairs`](super::node::NodeExt::pairs), which walks
    /// *child nodes*: `next="Next"` is a KDL parse error, not a table entry. The
    /// line before the block may carry the flag that turns its section on.
    Table,
    /// A nested block, whose own keys are these.
    Block(Rows),
    /// A block of repeated child nodes, each named by the author and each
    /// accepting these keys.
    Items(Rows),
    /// One node carrying these keys as `key=value` attributes on its own line,
    /// not as a block: `png level=6 strip="all"`.
    Line(Rows),
    /// Repeated nodes, each named by the author and each carrying these keys as
    /// `key=value` attributes: one line per taxonomy, per icon.
    Lines(Rows),
    /// A block of repeated child nodes, each named by the author and each
    /// holding a free [`Table`](Kind::Table) of its own: both levels are the
    /// author's, `headers { rules { "/v*/*" { X-Robots-Tag "noindex" } } }`.
    ///
    /// Unlike [`Items`](Kind::Items), neither level is a name this crate knows,
    /// so there are no rows for the reference to walk into.
    Tables,
    /// A block of repeated child nodes, each named by the author and each
    /// accepting *any top-level key*.
    ///
    /// Its own variant rather than [`Kind::Items(Config::rows)`](Kind::Items),
    /// which would send the reference walker into an infinite recursion.
    Overlay,
}

impl Kind {
    /// What a key of this shape reads from the entries written on its own line,
    /// read out of the same column the reference is generated from.
    fn takes(self) -> Arity {
        match self {
            Self::Text
            | Self::Flag
            | Self::Number
            | Self::Size
            | Self::Time
            | Self::Version
            | Self::Level(_)
            | Self::Path
            | Self::Asset
            | Self::Url
            | Self::Template
            | Self::Choice(_) => Arity::Args(1),
            Self::Choices(_) | Self::Texts | Self::Numbers | Self::Toggles => Arity::Every,
            Self::Items(_) | Self::Lines(_) | Self::Overlay | Self::Table | Self::Tables => {
                Arity::Args(0)
            }
            Self::Block(_) | Self::Line(_) | Self::Toggled(..) => Arity::Elsewhere,
        }
    }
}

/// What a key reads from the entries written on its own line, derived from
/// [`Kind`] so a key states its shape once.
#[derive(Clone, Copy)]
pub(super) enum Arity {
    /// At most this many positional arguments, and never a `key=value`.
    /// `Args(1)` is every scalar key; `Args(0)` a line whose settings all live
    /// in the block beneath it.
    Args(usize),
    /// Any number of positional arguments, and never a `key=value`.
    Every,
    /// Not checked here: the entries belong to a reader that checks them itself
    /// ([`Attrs::apply`] for an attribute scope, [`Section::fill`] or
    /// [`Section::shorthand`] for a section).
    Elsewhere,
}

impl Arity {
    /// Refuse every entry on `node`'s own line that a key of this shape does
    /// not read, so a value nothing reads is never accepted and discarded.
    pub(super) fn check(self, node: &KdlNode, text: &str) -> Result<()> {
        if matches!(self, Self::Elsewhere) {
            return Ok(());
        }
        let name = node.name().value();
        for entry in node.entries() {
            let Some(key) = entry.name().map(KdlIdentifier::value) else {
                continue;
            };
            let span = EntryExt::span(entry);
            return Err(match self {
                Self::Every => ConfigError::unexpected_argument(
                    text,
                    &format!("{key}={}", Kdl(entry.value())),
                    name,
                    span,
                )
                .into(),
                _ => ConfigError::unexpected_attribute(
                    text,
                    key,
                    name,
                    &format!("{name} {{ {key} {} }}", Kdl(entry.value())),
                    span,
                )
                .into(),
            });
        }
        let positional = node.entries().iter().filter(|e| e.name().is_none());
        for (read, entry) in positional.enumerate() {
            if !self.reads(read) {
                return Err(self.refuse(text, name, entry.value(), EntryExt::span(entry)));
            }
        }
        Ok(())
    }

    /// Whether the positional argument at `index` is read by anything.
    fn reads(self, index: usize) -> bool {
        match self {
            Self::Args(takes) => index < takes,
            Self::Every | Self::Elsewhere => true,
        }
    }

    /// The diagnostic for a positional nothing reads: a section takes no value
    /// at all, while a scalar key has been handed a second one.
    fn refuse(
        self,
        text: &str,
        node: &str,
        value: &KdlValue,
        span: SourceSpan,
    ) -> BaudelaireErrorKind {
        let written = Kdl(value).to_string();
        match self {
            Self::Args(0) => ConfigError::unexpected_section_argument(
                text,
                &written,
                node,
                &format!("{node} {{ .. }}"),
                span,
            )
            .into(),
            _ => ConfigError::extra_argument(text, &written, node, span).into(),
        }
    }
}

/// A scope's documented rows, as a function rather than a slice so a section can
/// name its children without this module knowing their Rust types.
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
    /// The rows both [`Section`] and [`Attributed`] hand to the reference.
    fn of<R, W>(table: &'static [(&'static str, Kind, &'static str, R, W)]) -> Vec<Self> {
        table
            .iter()
            .map(|&(key, kind, doc, ..)| Self { key, kind, doc })
            .collect()
    }
}

/// A node-keyed scope (child nodes matched by name), e.g. the top-level config
/// or a `serve { ... }` block.
pub(super) struct Block<T: 'static>(pub(super) &'static [Rule<T>]);

impl<T> Block<T> {
    /// Apply this scope's rules to every node in `nodes`, erroring on the first
    /// unrecognized key (with a nearest-match suggestion).
    pub(super) fn apply(&self, value: &mut T, nodes: &[KdlNode], text: &str) -> Result<()> {
        for node in nodes {
            self.one(value, node.name().value(), node, text)?;
        }
        Ok(())
    }

    /// Apply the rule named `key` to `node`. Usually the node's own name, but a
    /// shorthand ([`Section::shorthand`]) hands a node to the rule for the key
    /// it stands in for, so the two spellings run the very same handler.
    fn one(&self, value: &mut T, key: &str, node: &KdlNode, text: &str) -> Result<()> {
        match self.0.iter().find(|(k, ..)| *k == key) {
            Some((_, kind, _, _, handler)) => {
                kind.takes().check(node, text)?;
                handler(value, node, text)
            }
            None => Err(Keys::unknown_key(self.0, text, key, NodeExt::span(node))),
        }
    }

    /// This scope's keys, as the reference renders them.
    fn rows(&self) -> Vec<Row> {
        Row::of(self.0)
    }

    /// Every key of this scope read off `value`, in table order.
    fn values(&self, value: &T) -> Vec<(String, Value)> {
        self.0
            .iter()
            .map(|&(key, _, _, read, _)| (key.to_owned(), read(value)))
            .collect()
    }
}

/// A config section: a struct filled from a node's `{ .. }` block, whose
/// [`RULES`](Section::RULES) table is the single source of truth for the keys
/// that block accepts. The fill-in-place, presence-enables and optional-backend
/// policies are written once here rather than once per section.
pub(super) trait Section: Sized + 'static {
    /// This section's `(key, kind, doc, handler)` table.
    const RULES: Block<Self>;

    /// This section's keys, as the reference renders them.
    ///
    /// A `fn() -> Vec<Row>` and not a constant, so a parent naming a child
    /// writes [`Kind::Block(Child::rows)`](Kind::Block) and never repeats the
    /// child's key list.
    fn rows() -> Vec<Row> {
        Self::RULES.rows()
    }

    /// What this section holds, key by key: the read half of the same table
    /// that parsed it.
    ///
    /// A switchable section that is off carries the boolean its own line would,
    /// its keys still readable beneath it.
    fn values(&self) -> Value {
        let block = Value::block(Self::RULES.values(self));
        match Self::SWITCH {
            Some(switch) if !(switch.on)(self) => block.with(vec![Value::Flag(false)]),
            _ => block,
        }
    }

    /// How many leading positional arguments the *caller* reads itself before
    /// the block is dispatched: a collection's glob, and nothing else so far.
    /// [`Section::line`] refuses every other entry on the line.
    const LEADING: usize = 0;

    /// The flag this section's own presence turns on, for a section that has
    /// one: `check` enables linting, and `check #false` takes it back off again.
    ///
    /// Declared as the setter so that "this section has a switch" and "here is
    /// the field it sets" are one statement. Off has to be sayable: a profile
    /// overlays nodes onto the base and the config language has no spelling for
    /// deleting one, so presence alone could never be taken back.
    const SWITCH: Option<Switch<Self>> = None;

    /// Refuse whatever a section's own line carries past the arguments the
    /// section itself reads: the [`LEADING`](Section::LEADING) ones its caller
    /// consumes, plus the [`SWITCH`](Section::SWITCH) boolean where there is
    /// one.
    ///
    /// Called by [`Section::fill`], and by a caller that reads the line itself
    /// before deciding whether there is a block to fill from at all.
    fn line(node: &KdlNode, text: &str) -> Result<()> {
        Arity::Args(Self::LEADING + usize::from(Self::SWITCH.is_some())).check(node, text)
    }

    /// Run before a block's keys are applied, with `on` read off the section's
    /// own line (a bare node is `#true`). A section turned on by the mere
    /// presence of its block sets its flag here and returns `true`, which is
    /// what lets a bare node with no `{ }` mean "just turn it on".
    ///
    /// Not run for a block an [`Overlaying`] pass names without a boolean: there
    /// the switch is a value the layer below decided.
    ///
    /// Overridden only where presence records something the line's boolean is
    /// *not* (`MarkdownConfig::present`, `PdfBundle::present`).
    fn enable(&mut self, on: bool) -> bool {
        let Some(switch) = Self::SWITCH else {
            return false;
        };
        (switch.set)(self, on);
        true
    }

    /// Apply a node's `{ .. }` children onto `self`, *filling in place*: a key
    /// the block omits keeps the value it already had, which is what lets a
    /// profile override one key of a section and inherit its siblings.
    ///
    /// A node with no block at all is the "presence is the switch" spelling, and
    /// is accepted only where there is a switch to flip.
    fn fill(&mut self, node: &KdlNode, text: &str) -> Result<()> {
        Self::line(node, text)?;
        let flips = node.get(Self::LEADING).is_some()
            || node.children().is_none()
            || !Overlaying::active();
        let switch = flips && self.enable(node.boolean(text, Self::LEADING)?);
        match node.children() {
            Some(block) => Self::RULES.apply(self, block.nodes(), text),
            None if switch => Ok(()),
            None => Err(ConfigError::missing_children(text, NodeExt::span(node)).into()),
        }
    }

    /// Fill a section that also answers to a bare value, which is read as the
    /// key `stands_for`: `drafts #true` is `drafts { build #true }`. The
    /// argument reaches that key's own handler untouched, so the shorthand
    /// accepts and refuses exactly what the long spelling does.
    ///
    /// A block may still follow the argument, and either alone is enough.
    ///
    /// The section is enabled unconditionally: the line's own boolean belongs to
    /// `stands_for`'s handler, and [`enable`](Section::enable) records only that
    /// the section was named at all.
    fn shorthand(&mut self, node: &KdlNode, text: &str, stands_for: &'static str) -> Result<()> {
        self.enable(true);
        match node.children() {
            Some(block) => {
                if !node.entries().is_empty() {
                    Self::RULES.one(self, stands_for, node, text)?;
                }
                Self::RULES.apply(self, block.nodes(), text)
            }
            None => Self::RULES.one(self, stands_for, node, text),
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

    /// This item's attributes, as the reference renders them.
    fn rows() -> Vec<Row> {
        Self::ATTRS.rows()
    }

    /// What this item holds: the leading positionals no attribute of its own
    /// reports, then every attribute.
    fn values(&self) -> Value {
        Value::line(self.unkeyed(), Self::ATTRS.values(self))
    }

    /// The leading positionals the caller reads itself and no attribute reports,
    /// read back. [`LEADING`](Attributed::LEADING) says how many a line carries;
    /// this says what they hold, for the items whose own value is one of them.
    fn unkeyed(&self) -> Vec<Value> {
        Vec::new()
    }

    /// How many leading positional arguments the caller consumes itself (a
    /// collection's glob); any other positional is an error.
    const LEADING: usize = 0;

    /// Whether the caller reads the node's `{ .. }` block itself, as a schema
    /// field does for the fields of a dictionary. Otherwise a block is refused:
    /// [`Attrs::apply`] reads only entries, so anything inside braces would
    /// parse and configure nothing.
    const NESTS: bool = false;

    /// Apply the node's named attributes onto `self`.
    fn read(&mut self, node: &KdlNode, text: &str) -> Result<()> {
        if !Self::NESTS && node.children().is_some() {
            let name = node.name().value();
            return Err(ConfigError::unexpected_block(
                text,
                name,
                &Self::ATTRS.example(name),
                NodeExt::span(node),
            )
            .into());
        }
        Self::ATTRS.apply(self, node, text, Self::LEADING)
    }
}

/// An attribute-keyed scope (a node's `key=value` entries), e.g. a single
/// `content { taxonomies { tags { .. } } }` block. Same contract as
/// [`Block`], but handlers receive the attribute value.
pub(super) struct Attrs<T: 'static>(pub(super) &'static [Attr<T>]);

impl<T> Attrs<T> {
    /// Apply named attributes of `node`, erroring on the first unrecognized
    /// attribute. At most `leading` positional entries are tolerated, and only
    /// at the front of the node, since the caller consumes those itself.
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
                        &Kdl(entry.value()).to_string(),
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

    /// Every attribute of this scope read off `value`, in table order.
    pub(super) fn values(&self, value: &T) -> Vec<(String, Value)> {
        self.0
            .iter()
            .map(|&(key, _, _, read, _)| (key.to_owned(), read(value)))
            .collect()
    }

    /// The node written the way it parses, for the diagnostic that refuses a
    /// block, read out of the same table so the spelling it shows works.
    fn example(&self, node: &str) -> String {
        match self.0.first() {
            Some(&(key, kind, ..)) => format!("{node} {key}={}", kind.label()),
            None => node.to_owned(),
        }
    }
}

/// The valid keys of a scope, derived from its dispatch table, building
/// "unknown key" errors that carry a nearest-match hint.
pub(crate) struct Keys<'a>(pub(super) &'a [&'a str]);

impl<'a> Keys<'a> {
    /// The "closest known name" helper, reused wherever a typo should suggest a
    /// valid name (config keys, frontmatter fields).
    pub(crate) fn of(names: &'a [&'a str]) -> Self {
        Self(names)
    }
}

impl Keys<'_> {
    /// Build an unknown-*key* error (a structural node/attribute name) from any
    /// dispatch `table`.
    pub(super) fn unknown_key<R, W>(
        table: &[(&'static str, Kind, &'static str, R, W)],
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
    /// The suggestion gets a line of its own and each valid name a code span:
    /// the break survives miette's wrapper, which re-indents the rest into the
    /// help column.
    pub(crate) fn help(&self, unknown: &str, noun: &str) -> String {
        let suggestion = self
            .nearest(unknown)
            .map_or_else(String::new, |near| markup!("did you mean `{}`?\n", near));
        let names = self.0.iter().map(Code).format(", ");
        format!("{suggestion}{}{names}", markup!("valid {}: ", noun))
    }

    /// The same help for a destination that does not read this crate's markup:
    /// a typst message, which comes back escaped, so a code span there is a
    /// pair of literal backticks and the line break lands mid-sentence.
    pub(crate) fn plainly(&self, unknown: &str, noun: &str) -> String {
        let suggestion = self
            .nearest(unknown)
            .map_or_else(String::new, |near| format!("did you mean {near}? "));
        format!("{suggestion}valid {noun}: {}", self.0.iter().format(", "))
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
        assert_eq!(
            Keys(&["pretty", "indent"]).help("pruty", "keys"),
            "did you mean `pretty`?\nvalid keys: `pretty`, `indent`"
        );
    }

    #[test]
    fn the_plain_help_carries_no_markup_and_stays_on_one_line() {
        let plain = Keys(&["pretty", "indent"]).plainly("pruty", "keys");
        assert_eq!(plain, "did you mean pretty? valid keys: pretty, indent");
        assert!(!plain.contains(['`', '\n']), "{plain}");
    }
}
