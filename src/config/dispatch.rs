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
use super::value::Kdl;

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

/// The setter a switchable [`Section`] names for the flag its own presence turns
/// on: see [`Section::SWITCH`].
pub(super) type Switch<T> = fn(&mut T, bool);

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
    /// One of a fixed set of names: `html "drop"`. Carried as a function over
    /// [`Named::names`](crate::config::Named::names) rather than as a literal
    /// list, so the names the reference prints are read out of the very table
    /// that parses them.
    Choice(Names),
    /// Any number of names from a fixed set, written on one line and replacing
    /// whatever the key held: `formats "rss" "atom"`.
    ///
    /// [`Choice`](Kind::Choice) for a list, carried the same way and for the
    /// same reason. Its own variant because the three keys of this shape were
    /// declared `Choice` and so documented themselves as taking *one* name,
    /// while [`NodeExt::mapped`](super::node::NodeExt::mapped) read every one
    /// the author wrote.
    Choices(Names),
    /// Any number of names from a fixed set, where `-name` removes one from the
    /// key's defaults: `extensions "math" "-tables"`.
    ///
    /// Named after [`NodeExt::toggled`](super::node::NodeExt::toggled), which
    /// reads it, and distinct from [`Choices`](Kind::Choices) in what a list
    /// means: there it replaces, here it amends. The second function names the
    /// ones that are on without being asked for, so the reference states the
    /// default set rather than leaving prose to restate it somewhere that can go
    /// stale.
    Toggled(Names, Names),
    /// The same `-name` grammar over an *open* set, where the names are not a
    /// table this crate owns: `typst { features "bundle" "-a11y-extras" }`.
    ///
    /// Separate from [`Toggled`](Kind::Toggled) because there is nothing to
    /// list, and separate from [`Texts`](Kind::Texts) because the leading `-`
    /// is grammar rather than part of the name: a key spelled in it that
    /// rendered as a plain string list would leave the reader to discover the
    /// prefix from prose.
    Toggles,
    /// Any number of strings on one line: `footnotes "article" "main"`.
    Texts,
    /// Any number of whole numbers on one line: `widths 480 960 1440`.
    Numbers,
    /// A block of free entries, the keys chosen by the author, one node each:
    /// `strings { next "Next"; prev "Previous" }`.
    ///
    /// Read by [`NodeExt::pairs`](super::node::NodeExt::pairs), which walks
    /// *child nodes*. Written `next="Next"` it is a KDL parse error rather than
    /// a table entry, and the label said `key=value` for a while: the one
    /// spelling this shape does not take.
    ///
    /// The line before the block may carry the flag that turns its section on
    /// (`html { highlight #false }`), which is what `Kind::takes` allows for it.
    /// The keys that read no flag demand a block, so a stray value there is
    /// already a missing-children error.
    Table,
    /// A nested block, whose own keys are these.
    Block(Rows),
    /// A block of repeated child nodes, each named by the author and each
    /// accepting these keys.
    Items(Rows),
    /// One node carrying these keys as `key=value` attributes on its own line,
    /// not as a block: `png level=6 strip="all"`.
    ///
    /// Distinct from [`Kind::Block`] because the two are read by different
    /// halves of this module ([`Attrs`] against [`Block`]), and a reference that
    /// called this one a block would be documenting a spelling that parses and
    /// configures nothing.
    Line(Rows),
    /// Repeated nodes, each named by the author and each carrying these keys as
    /// `key=value` attributes: one line per taxonomy, per icon.
    Lines(Rows),
    /// A block of repeated child nodes, each named by the author and each
    /// accepting *any top-level key*.
    ///
    /// Its own variant rather than [`Kind::Items(Config::rows)`](Kind::Items), which is
    /// what it means: that spelling is honest and would send the reference
    /// walker into an infinite recursion, since a profile can hold a `profiles`
    /// block of its own.
    Overlay,
}

impl Kind {
    /// What a key of this shape reads from the entries written on its own line.
    ///
    /// Read out of the very column the reference is generated from, so a key
    /// cannot document one shape and quietly accept another. It is not
    /// derivable from the handler, for the same reason [`Kind`] itself is not:
    /// `n.string(t, 0)` says what the handler *reads*, never what the node may
    /// carry.
    fn takes(self) -> Arity {
        match self {
            // Every scalar: one value, and a second one is nobody's. `Choice`
            // is one of these -- it names *one* of a set -- and was exempt only
            // for as long as three multi-value keys were declared with it.
            // `Table` is here too, since its free-form block may be preceded by
            // the flag that turns a section on (see [`Kind::Table`]).
            Self::Text
            | Self::Flag
            | Self::Number
            | Self::Size
            | Self::Path
            | Self::Url
            | Self::Template
            | Self::Choice(_)
            | Self::Table => Arity::Args(1),
            // A list written on one line, however long.
            Self::Choices(_) | Self::Toggled(..) | Self::Toggles | Self::Texts | Self::Numbers => {
                Arity::Every
            }
            // A block of author-named children -- blocks, attribute lines, or a
            // profile overlay -- so the line opening it says nothing at all.
            // `Lines` is here and not below with `Line`: the entries
            // [`Attrs::apply`] reads belong to the *children*, and the parent
            // node's own were read by nobody.
            Self::Items(_) | Self::Lines(_) | Self::Overlay => Arity::Args(0),
            // Somebody else's line. A section owns its own -- [`Section::line`]
            // allows exactly what the section reads there (its
            // [`SWITCH`](Section::SWITCH), a collection's glob) and refuses the
            // rest, and [`Section::shorthand`] hands them to the key it stands
            // for -- and a single attribute line is read entry by entry by
            // [`Attrs::apply`], which refuses the ones it does not know.
            Self::Block(_) | Self::Line(_) => Arity::Elsewhere,
        }
    }
}

/// What a key reads from the entries written on its own line: the question
/// [`Block`] has to answer before it can refuse the ones nothing reads.
///
/// Derived from [`Kind`] rather than declared beside it, so a key states its
/// shape once and this follows.
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
    /// not read.
    ///
    /// The [`Block`] counterpart of the check [`Attrs::apply`] has always run,
    /// and the reason it had to exist: a node-keyed rule dispatched on the name
    /// alone and never looked at the line, so every value written there was
    /// accepted and discarded. `lint #false` turned linting *on* (it is now the
    /// spelling that turns it off), `serve { port 1 2 }` dropped the `2`, and
    /// `content { drafts suffix=".x" }` configured nothing while reporting
    /// nothing.
    pub(super) fn check(self, node: &KdlNode, text: &str) -> Result<()> {
        if matches!(self, Self::Elsewhere) {
            return Ok(());
        }
        let name = node.name().value();
        // A list key is the one shape that cannot be told to move the pair one
        // nesting level in: it has no block to move it into, and `extensions {
        // tables #false }` is not a line this config language has. Its own
        // reader already refuses a `key=value` with a message shaped like the
        // values it does take (`NodeExt::toggled`), so it is left to speak.
        // Everything else here either opens a block or stands in front of one.
        if !matches!(self, Self::Every) {
            for entry in node.entries() {
                let Some(key) = entry.name().map(KdlIdentifier::value) else {
                    continue;
                };
                // The help writes the line the author meant, which is the same
                // pair one nesting level in -- and through `Kdl`, so it is a
                // line that parses.
                return Err(ConfigError::unexpected_attribute(
                    text,
                    key,
                    name,
                    &format!("{name} {{ {key} {} }}", Kdl(entry.value())),
                    EntryExt::span(entry),
                )
                .into());
            }
        }
        // Positionals are counted among themselves: a named entry left for
        // someone else to refuse must not shift the index of the ones after it.
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

    /// The diagnostic for a positional nothing reads. The two cases differ in
    /// what they advise: a section takes no value at all and is configured from
    /// its block, while a scalar key has simply been handed a second value.
    fn refuse(
        self,
        text: &str,
        node: &str,
        value: &KdlValue,
        span: SourceSpan,
    ) -> BaudelaireErrorKind {
        // Echoed as the author wrote it, quotes and all: a value the message
        // spells differently from the line above it reads as a second value.
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
            self.one(value, node.name().value(), node, text)?;
        }
        Ok(())
    }

    /// Apply the rule named `key` to `node`. Usually the node's own name, but a
    /// shorthand ([`Section::shorthand`]) hands a node to the rule for the key
    /// it stands in for, so the two spellings run the very same handler.
    fn one(&self, value: &mut T, key: &str, node: &KdlNode, text: &str) -> Result<()> {
        match self.0.iter().find(|(k, ..)| *k == key) {
            // The line is checked before the handler runs: a handler reads the
            // arguments it wants by index and cannot see the ones it does not,
            // so nothing but the table knows what the key accepts.
            Some((_, kind, _, handler)) => {
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

    /// How many leading positional arguments the *caller* reads itself before
    /// the block is dispatched: a collection's glob, and nothing else so far.
    /// Every other entry on the line is read by nobody, so [`Section::line`]
    /// refuses it. The [`Attributed::LEADING`] counterpart, and for the same
    /// reason.
    const LEADING: usize = 0;

    /// The flag this section's own presence turns on, for a section that has
    /// one: `lint` enables linting, and `lint #false` takes it back off again.
    ///
    /// Declared as the setter rather than as a `bool` beside an
    /// [`enable`](Section::enable) override, so that "this section has a switch"
    /// and "here is the field it sets" are one statement rather than two that
    /// can disagree. [`Section::line`] reads the first half to decide whether
    /// the line may carry a boolean at all, and [`Section::enable`] the second
    /// to apply it.
    ///
    /// Off has to be sayable. Presence alone is a fine switch until a base
    /// config or a theme's `theme.kdl` names the section, at which point nothing
    /// downstream could take it back: a profile overlays nodes onto the base, so
    /// naming the section is what re-enables it, and the config language has no
    /// spelling for deleting a node.
    const SWITCH: Option<Switch<Self>> = None;

    /// Refuse whatever a section's own line carries past the arguments the
    /// section itself reads: the [`LEADING`](Section::LEADING) ones its caller
    /// consumes, plus the [`SWITCH`](Section::SWITCH) boolean where there is
    /// one.
    ///
    /// Called by [`Section::fill`], and by a caller that reads the line itself
    /// before deciding whether there is a block to fill from at all
    /// (`CollectionConfig::item`, where `posts sort="date"` has no block and so
    /// never reached `fill`).
    fn line(node: &KdlNode, text: &str) -> Result<()> {
        Arity::Args(Self::LEADING + usize::from(Self::SWITCH.is_some())).check(node, text)
    }

    /// Run before a block's keys are applied, with `on` read off the section's
    /// own line (a bare node is `#true`). A section that is turned on by the
    /// mere presence of its block sets its flag here and returns `true`, so that
    /// rule lives with the section rather than at every parent mentioning it.
    ///
    /// The return value is what lets a *bare* node with no `{ }` mean "just turn
    /// it on": the docs promise that `generate { robots }` enables robots.txt by
    /// existing, and it used to be a hard `missing_children` error instead.
    /// Reporting it from the same place that does the enabling is what keeps the
    /// two from disagreeing.
    ///
    /// Overridden only where presence records something the line's boolean is
    /// *not* (`MarkdownConfig::present`, `PdfBundle::present`); everything else
    /// names a [`SWITCH`](Section::SWITCH) and inherits this.
    fn enable(&mut self, on: bool) -> bool {
        let Some(set) = Self::SWITCH else {
            return false;
        };
        set(self, on);
        true
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
        // A section with no switch is configured from its block alone, so a
        // value on its line is read by nobody: `paths "junk"` and
        // `generate { robots "junk" }` are both refused here, the second as a
        // boolean it is not.
        Self::line(node, text)?;
        // Presence is the switch and the line is how it is taken back, so a
        // bare node reads as `#true` -- which is also what every section
        // without a switch reads, `line` having just refused it an argument.
        let switch = self.enable(node.boolean(text, Self::LEADING)?);
        match node.children() {
            Some(block) => Self::RULES.apply(self, block.nodes(), text),
            None if switch => Ok(()),
            None => Err(ConfigError::missing_children(text, NodeExt::span(node)).into()),
        }
    }

    /// Fill a section that also answers to a bare value, which is read as the
    /// key `stands_for`: `drafts #true` is `drafts { build #true }`. The
    /// argument reaches that key's own handler untouched, so the shorthand
    /// cannot accept a value the long spelling would refuse -- including what
    /// it refuses: the line is checked against `stands_for`'s own row, which is
    /// what makes `content { drafts suffix=".x" }` an error rather than a line
    /// that parses and configures nothing.
    ///
    /// A block may still follow the argument, and either alone is enough: the
    /// whole point is that the common case (one boolean) does not have to open
    /// braces to say it.
    fn shorthand(&mut self, node: &KdlNode, text: &str, stands_for: &'static str) -> Result<()> {
        // Unconditionally `true`, and never the line's own boolean: here that
        // value belongs to `stands_for`'s handler, and what `enable` records is
        // only that the section was named at all. The return value is what
        // `fill` needs and this does not, since a bare node is always legal
        // here.
        //
        // Without this, `content { markdown }` left `MarkdownConfig::present`
        // false in *every* spelling, because `markdown` is only ever dispatched
        // through here. The feature gate reads `present && enabled`, so a slim
        // binary dropped every `.md` page and said nothing at all.
        self.enable(true);
        match node.children() {
            Some(block) => {
                if !node.entries().is_empty() {
                    Self::RULES.one(self, stands_for, node, text)?;
                }
                Self::RULES.apply(self, block.nodes(), text)
            }
            // No block: the node itself is the value, and a bare `drafts` is
            // the "presence is the switch" spelling every flag already takes.
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

    /// This item's attributes, as the reference renders them. The [`Section`]
    /// counterpart, for the same reason.
    fn rows() -> Vec<Row> {
        Self::ATTRS.rows()
    }

    /// How many leading positional arguments the caller consumes itself (a
    /// collection's glob); any other positional is an error.
    const LEADING: usize = 0;

    /// Whether the caller reads the node's `{ .. }` block itself, as a schema
    /// field does for the fields of a dictionary. Otherwise a block on one of
    /// these nodes is refused: [`Attrs::apply`] reads only entries, so anything
    /// written inside braces would parse and configure nothing, which is the
    /// failure this whole dispatch layer exists to prevent.
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

    /// The node written the way it parses, for the diagnostic that refuses a
    /// block. Read out of the same table, so the spelling it shows is one that
    /// works and cannot drift from the keys.
    fn example(&self, node: &str) -> String {
        match self.0.first() {
            Some(&(key, kind, ..)) => format!("{node} {key}={}", kind.label()),
            None => node.to_owned(),
        }
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
