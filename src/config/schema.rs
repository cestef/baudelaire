//! The shapes a collection's frontmatter schema declares, and the small type
//! language that spells them.
//!
//! A field's type is an expression, not a keyword: `list<..>` wraps another
//! type, so `list<int>`, `list<list<int>>` and `list<dict>` all say exactly what
//! they hold. The leaves are the Typst types a page can write in a frontmatter
//! dict, never a second type system.

use kdl::KdlNode;
use miette::SourceSpan;

use dispatch_derive::Table;

use crate::config::Value;
use crate::config::dispatch::Kind;
use crate::config::dispatch::Kind::Choice;
use crate::config::dispatch::{Attributed, Attrs, Keys};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::vocab::attr;
use crate::content::Frontmatter;
use crate::error::{BaudelaireErrorKind, ConfigError, ConfigErrorKind, Result};
use crate::ui::markup;

/// How a type is named to a reader, in every shape a diagnostic needs it: `a
/// string`, `strings`, `a list of strings`.
///
/// Static because a miette label is not markup-rendered and so may only carry
/// this crate's own literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Words {
    /// `a string`
    pub article: &'static str,
    /// `strings`
    pub plural: &'static str,
    /// `a list of strings`
    pub list: &'static str,
}

/// One row of [`FieldType::words`], from the two words that are not derivable
/// from each other; the list form is concatenated from the plural.
macro_rules! words {
    ($article:literal, $plural:literal) => {
        Words {
            article: $article,
            plural: $plural,
            list: concat!("a list of ", $plural),
        }
    };
}

/// One field a collection's frontmatter schema declares.
///
/// Declaring a field *requires* it; a field that may be absent says so with
/// `optional=#true`.
#[derive(Debug, Clone, Default, Hash, PartialEq, Table)]
#[table(
    impl = Attributed,
    const ATTRS: Attrs<Self> = Attrs,
    rule = attr,
    items {
        /// The type, written as the leading positional.
        const LEADING: usize = 1;

        /// `item` reads the block as the fields of the dictionary the type ends in.
        const NESTS: bool = true;

        /// A dictionary's own fields are written in this line's block, so they read
        /// back as keys of it.
        fn values(&self) -> crate::config::Value {
            let keys = self.ty.fields().map_or_else(Vec::new, |fields| {
                fields
                    .iter()
                    .map(|(name, field)| (name.clone(), field.values()))
                    .collect()
            });
            crate::config::Value::nested(self.unkeyed(), Self::ATTRS.values(self), keys)
        }
    },
)]
pub struct FieldSchema {
    /// The shape the value must have, also writable as the leading positional: `title "str"`. A list names what it holds: `list<int>`, `list<dict>`.
    ///
    /// [`FieldType::Any`] (the default, and what a bare `hero` means)
    /// constrains only presence.
    #[key(name = "type", custom(
        Choice(FieldType::names),
        |c: &Self| Value::written(&c.ty),
        |c: &mut Self, v: &kdl::KdlValue, t: &str, s: miette::SourceSpan| {
            c.ty = v.ty(t, s)?;
            Ok(())
        },
    ))]
    pub ty: FieldType,

    /// Let the field be absent. Declaring a field otherwise requires it.
    #[key(flag)]
    pub optional: bool,

    /// What the page gets when it writes none, which lets the field be absent.
    ///
    /// A scalar of the declared type, so a `list` and a `dict` have none: a
    /// default nobody can write in one line is one nobody can read either.
    #[key(custom(
        Kind::Text,
        FieldSchema::written,
        |c: &mut Self, v: &kdl::KdlValue, t: &str, s: miette::SourceSpan| {
            c.default = Some(v.scalar(t, s)?);
            Ok(())
        },
    ))]
    pub default: Option<crate::codegen::Value>,

    /// The floor the value is held to: its own for a number, its length for a string, its size for a list.
    #[key(opt int)]
    pub min: Option<i64>,

    /// The ceiling, read the same way as `min`.
    #[key(opt int)]
    pub max: Option<i64>,
}

/// Written back as the type expression a config line spells.
impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::List(inner) => write!(f, "{}<{inner}>", Self::LIST),
            Self::OneOf(values) => {
                write!(
                    f,
                    "{}<{}>",
                    Self::ONE_OF,
                    values.join(&Self::OR.to_string())
                )
            }
            Self::Dict(_) => f.write_str("dict"),
            leaf => f.write_str(leaf.leaf_name().unwrap_or("any")),
        }
    }
}

/// What a field's `min` and `max` hold: the one place a bound's meaning and the
/// words a diagnostic reports it in are stated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    /// The number itself.
    Value,
    /// How many characters the string has.
    Length,
    /// How many elements the list has.
    Items,
}

impl Bound {
    /// What `n` counts, as a clause reads it: `3`, `3 characters`, `3 items`.
    pub fn counted(self, n: i64) -> String {
        match self {
            Self::Value => n.to_string(),
            Self::Length if n == 1 => format!("{n} character"),
            Self::Length => format!("{n} characters"),
            Self::Items if n == 1 => format!("{n} item"),
            Self::Items => format!("{n} items"),
        }
    }

    /// Whether `value` sits between the floor and the ceiling a field declared.
    ///
    /// A value this kind of bound does not apply to fits: the type check has
    /// already refused it, and a second complaint about the same value would
    /// only bury the first.
    pub fn fits(
        self,
        value: &typst::foundations::Value,
        min: Option<i64>,
        max: Option<i64>,
    ) -> bool {
        let Some(measured) = self.measure(value) else {
            return true;
        };
        min.is_none_or(|floor| measured >= Self::at(floor))
            && max.is_none_or(|ceiling| measured <= Self::at(ceiling))
    }

    /// What `value` measures.
    ///
    /// A float measures as itself rather than as its whole part, so `max=1`
    /// refuses `1.5` instead of truncating it into a number that passes.
    fn measure(self, value: &typst::foundations::Value) -> Option<f64> {
        use typst::foundations::Value;
        match (self, value) {
            (Self::Value, Value::Int(n)) => Some(Self::at(*n)),
            (Self::Value, Value::Float(n)) => Some(*n),
            (Self::Length, Value::Str(text)) => i64::try_from(text.as_str().chars().count())
                .ok()
                .map(Self::at),
            (Self::Items, Value::Array(items)) => i64::try_from(items.len()).ok().map(Self::at),
            _ => None,
        }
    }

    /// A bound on the scale a measurement is compared on.
    ///
    /// Exact for every bound a schema can sensibly declare; a length or a count
    /// beyond 2^53 is not one.
    #[allow(clippy::cast_precision_loss)]
    fn at(n: i64) -> f64 {
        n as f64
    }
}

/// The shape a schema field requires of a frontmatter value.
///
/// These are the Typst types a page can write in a frontmatter dict: `date` is
/// `datetime(..)`, `dict` a dictionary, and `list<T>` an array whose every
/// element is a `T`.
#[derive(Debug, Clone, PartialEq, Hash, Default)]
pub enum FieldType {
    /// Any value at all: the field must merely be there.
    #[default]
    Any,
    Str,
    Bool,
    Int,
    Float,
    /// A `datetime(..)`, with or without a time of day.
    Date,
    /// One of a fixed set of strings, in the order they were declared.
    OneOf(Vec<String>),
    /// An array whose every element has this type. Bare `list` is `list<str>`.
    List(Box<Self>),
    /// A dictionary, and the fields it must carry. Empty (a bare `dict`)
    /// constrains the shape and nothing inside it.
    Dict(Vec<(String, FieldSchema)>),
}

impl FieldType {
    /// The two constructor spellings: what a list holds, and what a choice
    /// allows.
    const LIST: &'static str = "list";
    const ONE_OF: &'static str = "one-of";

    /// The byte between a choice's values.
    const OR: char = '|';

    /// The leaf types, in the order the reference lists them: what the
    /// innermost name of a type expression may be.
    fn leaves() -> Vec<(&'static str, Self)> {
        vec![
            ("any", Self::Any),
            ("str", Self::Str),
            ("bool", Self::Bool),
            ("int", Self::Int),
            ("float", Self::Float),
            ("date", Self::Date),
            ("dict", Self::Dict(Vec::new())),
        ]
    }

    /// Every spelling the type language accepts, for the generated reference
    /// and for the "valid values" help on a typo.
    pub fn names() -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Self::leaves().into_iter().map(|(n, _)| n).collect();
        names.push(Self::LIST);
        names.push("list<..>");
        names.push("one-of<..>");
        names
    }

    /// The values a choice allows, or `None` for a type that is not one.
    pub fn choices(&self) -> Option<&[String]> {
        match self {
            Self::OneOf(values) => Some(values),
            _ => None,
        }
    }

    /// What a `min` or a `max` on a field of this type holds, or `None` for a
    /// type with nothing to count or compare.
    pub fn bound(&self) -> Option<Bound> {
        match self {
            Self::Int | Self::Float => Some(Bound::Value),
            Self::Str => Some(Bound::Length),
            Self::List(_) => Some(Bound::Items),
            // A choice already enumerates what it allows; bounding the length
            // of one of its own values says nothing.
            Self::Any | Self::Bool | Self::Date | Self::OneOf(_) | Self::Dict(_) => None,
        }
    }

    /// The type a config string spells.
    ///
    /// Iterative over the `list<` prefixes rather than recursive, so a config
    /// with an absurd nesting depth is an error and never a blown stack.
    pub fn parse(src: &str) -> Result<Self, TypeError> {
        let mut name = src.trim();
        let mut depth = 0usize;
        while let Some(rest) = name.strip_prefix(Self::LIST) {
            let rest = rest.trim_start();
            if rest.is_empty() {
                name = "str";
                depth += 1;
                break;
            }
            let Some(open) = rest.strip_prefix('<') else {
                break;
            };
            name = open
                .strip_suffix('>')
                .ok_or_else(|| TypeError::Malformed(src.trim().to_owned()))?
                .trim();
            depth += 1;
        }
        let mut ty = Self::leaf(name, src)?;
        for _ in 0..depth {
            ty = Self::List(Box::new(ty));
        }
        Ok(ty)
    }

    /// One leaf name. A name carrying an angle bracket is a broken expression
    /// rather than an unknown type: `list<int>>` misspells no leaf.
    fn leaf(name: &str, src: &str) -> Result<Self, TypeError> {
        if let Some(rest) = name.strip_prefix(Self::ONE_OF) {
            return Self::choice(rest.trim_start(), src);
        }
        if name.is_empty() || name.contains(['<', '>']) {
            return Err(TypeError::Malformed(src.trim().to_owned()));
        }
        Self::leaves()
            .into_iter()
            .find(|(known, _)| *known == name)
            .map(|(_, ty)| ty)
            .ok_or_else(|| TypeError::Unknown(name.to_owned()))
    }

    /// The values a `one-of<a|b>` allows, in the order they were written.
    ///
    /// Strings, and only strings: a set of numbers is a range, which is what
    /// `min` and `max` are for. An empty value, or the same one twice, is a
    /// broken expression rather than a choice nobody can satisfy.
    fn choice(rest: &str, src: &str) -> Result<Self, TypeError> {
        let inside = rest
            .strip_prefix('<')
            .and_then(|open| open.strip_suffix('>'))
            .ok_or_else(|| TypeError::Malformed(src.trim().to_owned()))?;
        let values: Vec<String> = inside
            .split(Self::OR)
            .map(|value| value.trim().to_owned())
            .collect();
        let distinct = |value: &String| values.iter().filter(|other| *other == value).count() == 1;
        if values.iter().any(String::is_empty) || !values.iter().all(distinct) {
            return Err(TypeError::Malformed(src.trim().to_owned()));
        }
        Ok(Self::OneOf(values))
    }

    /// The fields the dictionary this type ends in declares, however many
    /// `list<..>` wrap it.
    pub fn fields(&self) -> Option<&Vec<(String, FieldSchema)>> {
        match self {
            Self::Dict(fields) => Some(fields),
            Self::List(inner) => inner.fields(),
            _ => None,
        }
    }

    /// The dictionary this type ends in, if it ends in one: where the fields of
    /// a nested block attach, however many `list<..>` wrap it.
    pub fn fields_mut(&mut self) -> Option<&mut Vec<(String, FieldSchema)>> {
        match self {
            Self::Dict(fields) => Some(fields),
            Self::List(inner) => inner.fields_mut(),
            _ => None,
        }
    }

    /// The leaf name this type is spelled with, for the ones that have one.
    fn leaf_name(&self) -> Option<&'static str> {
        Self::leaves()
            .into_iter()
            .find(|(_, leaf)| leaf == self)
            .map(|(name, _)| name)
    }

    /// The English a diagnostic names this type by.
    ///
    /// A list is named after what it holds, so [`article`](Self::article) and
    /// [`plural`](Self::plural) answer for one before reaching here.
    pub fn words(&self) -> Words {
        match self {
            Self::Any => words!("any value", "values"),
            // A choice is a string that names itself; both read as one.
            Self::Str | Self::OneOf(_) => words!("a string", "strings"),
            Self::Bool => words!("a boolean", "booleans"),
            Self::Int => words!("an integer", "integers"),
            Self::Float => words!("a float", "floats"),
            Self::Date => words!("a date", "dates"),
            Self::Dict(_) => words!("a dictionary", "dictionaries"),
            Self::List(_) => words!("a list", "lists"),
        }
    }

    /// How a diagnostic names this type ("a string").
    pub fn article(&self) -> String {
        match self {
            Self::List(inner) => format!("a list of {}", inner.plural()),
            Self::OneOf(values) => format!("one of {}", Self::quoted(values)),
            _ => self.words().article.to_owned(),
        }
    }

    /// A choice's values as a message names them.
    ///
    /// Double quotes rather than backticks: a message is styled as markup
    /// before it is shown, and these values are the site's own text.
    fn quoted(values: &[String]) -> String {
        values
            .iter()
            .map(|value| format!("\"{value}\""))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// How [`article`](Self::article) names this type inside a list, so a nested
    /// one reads as "a list of lists of integers" rather than stacking articles.
    fn plural(&self) -> String {
        match self {
            Self::List(inner) => format!("lists of {}", inner.plural()),
            _ => self.words().plural.to_owned(),
        }
    }

    /// A Typst literal of this type, for the "add the field" help. A declared
    /// dictionary shows the fields it requires.
    pub fn example(&self) -> String {
        match self {
            Self::Any | Self::Str => "\"..\"".to_owned(),
            Self::Bool => "false".to_owned(),
            Self::Int => "0".to_owned(),
            Self::Float => "0.0".to_owned(),
            Self::Date => "datetime(year: 2024, month: 1, day: 1)".to_owned(),
            Self::OneOf(values) => format!("\"{}\"", values.first().map_or("", String::as_str)),
            Self::List(inner) => format!("({},)", inner.example()),
            Self::Dict(fields) => {
                let required: Vec<String> = fields
                    .iter()
                    .filter(|(_, field)| !field.optional)
                    .map(|(key, field)| format!("{key}: {}", field.ty.example()))
                    .collect();
                if required.is_empty() {
                    "(:)".to_owned()
                } else {
                    format!("({})", required.join(", "))
                }
            }
        }
    }
}

/// A type expression that names no type.
///
/// Not a diagnostic itself: the config layer owns the span, and turns this into
/// one through [`TypeError::at`].
#[derive(Debug, PartialEq, Eq)]
pub enum TypeError {
    /// A leaf name no type answers to.
    Unknown(String),
    /// A `list<..>` that never closes, or wraps nothing.
    Malformed(String),
}

impl TypeError {
    /// This failure as a config diagnostic, underlining the value that wrote it.
    pub fn at(self, text: &str, span: SourceSpan) -> BaudelaireErrorKind {
        let names = FieldType::names();
        match self {
            Self::Unknown(name) => ConfigError::unknown_value(
                text,
                &name,
                Keys::of(&names).help(&name, "values"),
                span,
            )
            .into(),
            Self::Malformed(src) => ConfigError::at(
                text,
                ConfigErrorKind::TypeExpr {
                    ty: src,
                    help: markup!("a list names what it holds: `list<int>`, `list<dict>`"),
                },
                span,
            )
            .into(),
        }
    }
}

impl FieldSchema {
    /// Refuse a constraint the declared type cannot answer for.
    ///
    /// Caught here rather than at the page, where a bound nothing can measure
    /// would simply never fire and a default of the wrong type would be handed
    /// to every page that omitted the field.
    fn constrained(&self, key: &str, node: &KdlNode, text: &str) -> Result<()> {
        let span = NodeExt::span(node);
        let refuse = |kind| Err(ConfigError::at(text, kind, span).into());
        if (self.min.is_some() || self.max.is_some()) && self.ty.bound().is_none() {
            return refuse(ConfigErrorKind::FieldNotBounded {
                key: key.to_owned(),
                declared: self.ty.article(),
            });
        }
        if let (Some(floor), Some(ceiling)) = (self.min, self.max)
            && floor > ceiling
        {
            return refuse(ConfigErrorKind::FieldBoundsCross {
                key: key.to_owned(),
                min: floor,
                max: ceiling,
            });
        }
        if let Some(default) = &self.default {
            let value = typst::foundations::Value::from(default);
            if crate::content::Check::fits(&self.ty, &value) {
                return Ok(());
            }
            return refuse(ConfigErrorKind::FieldDefault {
                key: key.to_owned(),
                declared: self.ty.article(),
            });
        }
        Ok(())
    }

    /// The default as the reference reports it, in the shape the config wrote.
    fn written(&self) -> Value {
        self.default.as_ref().map_or(Value::Unset, Value::from)
    }

    /// One `title "str" optional=#true` line: the node name is the frontmatter
    /// key it constrains, and an optional leading positional its type.
    ///
    /// A `{ .. }` block declares the fields of the dictionary the type ends in,
    /// through however many `list<..>` wrap it:
    /// `authors "list<dict>" { name "str" }`.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let key = node.name().value().to_owned();
        let span = NodeExt::span(node);
        let mut field = Self::default();
        if let Some(value) = node.get(0) {
            field.ty = value.ty(text, span)?;
        }
        field.read(node, text)?;
        if node.children().is_some() {
            let fields = node.unique(text, "schema field", Self::item)?;
            let declared = field.ty.article();
            let Some(dict) = field.ty.fields_mut() else {
                return Err(ConfigError::at(
                    text,
                    ConfigErrorKind::FieldNotDict { key, declared },
                    span,
                )
                .into());
            };
            *dict = fields;
        }
        field.constrained(&key, node, text)?;
        if let Some(builtin) = Frontmatter::builtin(&key)
            && field.ty != FieldType::Any
            && field.ty != builtin
        {
            return Err(ConfigError::at(
                text,
                ConfigErrorKind::FieldConflict {
                    key,
                    declared: field.ty.article(),
                    builtin: builtin.article(),
                },
                span,
            )
            .into());
        }
        Ok((key, field))
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldType, TypeError};

    fn list(inner: FieldType) -> FieldType {
        FieldType::List(Box::new(inner))
    }

    #[test]
    fn parses_leaves_and_nested_lists() {
        assert_eq!(FieldType::parse("str"), Ok(FieldType::Str));
        assert_eq!(FieldType::parse("dict"), Ok(FieldType::Dict(Vec::new())));
        assert_eq!(FieldType::parse("list"), Ok(list(FieldType::Str)));
        assert_eq!(FieldType::parse("list<int>"), Ok(list(FieldType::Int)));
        assert_eq!(
            FieldType::parse("list<list<int>>"),
            Ok(list(list(FieldType::Int)))
        );
        assert_eq!(
            FieldType::parse(" list< dict > "),
            Ok(list(FieldType::Dict(Vec::new())))
        );
    }

    #[test]
    fn parses_a_choice_and_the_lists_that_hold_one() {
        let draft = || FieldType::OneOf(vec!["draft".to_owned(), "published".to_owned()]);
        assert_eq!(FieldType::parse("one-of<draft|published>"), Ok(draft()));
        assert_eq!(
            FieldType::parse(" one-of< draft | published > "),
            Ok(draft())
        );
        assert_eq!(
            FieldType::parse("list<one-of<draft|published>>"),
            Ok(list(draft()))
        );
    }

    /// A choice nobody could satisfy, and one that says the same thing twice,
    /// are both mistakes rather than types.
    #[test]
    fn rejects_a_choice_that_names_nothing_or_repeats_itself() {
        for src in [
            "one-of<>",
            "one-of<a|>",
            "one-of<|a>",
            "one-of<a|a>",
            "one-of",
            "one-of<a",
        ] {
            assert!(FieldType::parse(src).is_err(), "{src} should not be a type");
        }
    }

    /// The written form round-trips, so the reference and a diagnostic spell a
    /// type the way the config did.
    #[test]
    fn a_type_is_written_back_as_it_was_parsed() {
        for src in [
            "str",
            "list<int>",
            "list<list<int>>",
            "one-of<a|b>",
            "list<one-of<a|b>>",
        ] {
            let ty = FieldType::parse(src).expect("a valid type");
            assert_eq!(ty.to_string(), src);
        }
    }

    #[test]
    fn a_choice_names_its_values_and_offers_the_first() {
        let ty = FieldType::parse("one-of<draft|published>").expect("a valid type");
        assert_eq!(ty.article(), r#"one of "draft", "published""#);
        assert_eq!(ty.example(), r#""draft""#);
    }

    /// What a bound counts is the type's business, and a type with nothing to
    /// count declines one.
    #[test]
    fn a_bound_counts_what_the_type_has() {
        use crate::config::Bound;
        let bound = |src: &str| FieldType::parse(src).expect("a valid type").bound();

        assert_eq!(bound("int"), Some(Bound::Value));
        assert_eq!(bound("float"), Some(Bound::Value));
        assert_eq!(bound("str"), Some(Bound::Length));
        assert_eq!(bound("list<int>"), Some(Bound::Items));
        assert_eq!(bound("bool"), None);
        assert_eq!(bound("date"), None);
        assert_eq!(bound("dict"), None);
        assert_eq!(bound("one-of<a|b>"), None);
    }

    #[test]
    fn a_bound_reads_as_what_it_counts() {
        use crate::config::Bound;
        assert_eq!(Bound::Value.counted(3), "3");
        assert_eq!(Bound::Length.counted(1), "1 character");
        assert_eq!(Bound::Length.counted(3), "3 characters");
        assert_eq!(Bound::Items.counted(1), "1 item");
        assert_eq!(Bound::Items.counted(3), "3 items");
    }

    /// A float is measured as itself: truncating `1.5` to `1` would let it
    /// through a `max=1` it does not satisfy.
    #[test]
    fn a_float_is_bounded_by_its_whole_value() {
        use crate::config::Bound;
        use typst::foundations::Value;

        assert!(Bound::Value.fits(&Value::Float(1.0), None, Some(1)));
        assert!(!Bound::Value.fits(&Value::Float(1.5), None, Some(1)));
        assert!(!Bound::Value.fits(&Value::Float(0.5), Some(1), None));
    }

    /// A value the bound does not apply to fits: the type check has already
    /// refused it, and a second complaint would bury the first.
    #[test]
    fn a_bound_says_nothing_about_a_value_it_cannot_measure() {
        use crate::config::Bound;
        use typst::foundations::Value;

        assert!(Bound::Items.fits(&Value::Int(0), Some(5), None));
    }

    #[test]
    fn rejects_unknown_leaves_and_broken_expressions() {
        assert_eq!(
            FieldType::parse("strr"),
            Err(TypeError::Unknown("strr".to_owned()))
        );
        assert_eq!(
            FieldType::parse("listy"),
            Err(TypeError::Unknown("listy".to_owned()))
        );
        assert_eq!(
            FieldType::parse("list<int"),
            Err(TypeError::Malformed("list<int".to_owned()))
        );
        assert_eq!(
            FieldType::parse("list<>"),
            Err(TypeError::Malformed("list<>".to_owned()))
        );
        assert_eq!(
            FieldType::parse("list<int>>"),
            Err(TypeError::Malformed("list<int>>".to_owned()))
        );
    }

    /// The static list form and the one `article` composes must stay the same
    /// words: different readers use each, for the same failure.
    #[test]
    fn the_static_list_form_is_the_composed_one() {
        for ty in [
            FieldType::Str,
            FieldType::Bool,
            FieldType::Int,
            FieldType::Float,
            FieldType::Date,
        ] {
            assert_eq!(ty.words().list, list(ty.clone()).article(), "{ty:?}");
        }
    }

    #[test]
    fn names_a_nested_type_the_way_a_reader_would() {
        assert_eq!(list(FieldType::Int).article(), "a list of integers");
        assert_eq!(
            list(list(FieldType::Int)).article(),
            "a list of lists of integers"
        );
        assert_eq!(
            list(FieldType::Dict(Vec::new())).article(),
            "a list of dictionaries"
        );
    }

    #[test]
    fn an_example_shows_the_fields_a_dictionary_requires() {
        let ty = FieldType::parse("list<dict>").expect("a valid type");
        let FieldType::List(mut inner) = ty else {
            panic!("expected a list");
        };
        *inner.fields_mut().expect("a dict leaf") = vec![
            ("name".to_owned(), super::FieldSchema::default()),
            (
                "email".to_owned(),
                super::FieldSchema {
                    ty: FieldType::Str,
                    optional: true,
                    ..super::FieldSchema::default()
                },
            ),
        ];
        assert_eq!(list(*inner).example(), "((name: \"..\"),)");
    }
}
