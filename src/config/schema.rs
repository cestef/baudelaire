//! The shapes a collection's frontmatter schema declares, and the small type
//! language that spells them.
//!
//! A field's type is an expression, not a keyword: `list<..>` wraps another
//! type, so `list<int>`, `list<list<int>>` and `list<dict>` all say exactly what
//! they hold. The leaves are the Typst types a page can write in a frontmatter
//! dict, never a second type system. Written beside its own parser and error the
//! way [`Permalink`](crate::config::Permalink) is, because it is the same kind
//! of thing: a mini-language inside a config string.

use kdl::KdlNode;
use miette::SourceSpan;

use crate::config::dispatch::Kind::{Choice, Flag};
use crate::config::dispatch::{Attributed, Attrs, Keys};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::content::Frontmatter;
use crate::error::{BaudelaireErrorKind, ConfigError, ConfigErrorKind, Result};
use crate::ui::markup;

/// How a type is named to a reader, in every shape a diagnostic needs it: `a
/// string`, `strings`, `a list of strings`.
///
/// Static, and that is the point. A frontmatter type mismatch renders the
/// expected type in a miette *label*, which is not markup-rendered and so may
/// only ever carry this crate's own literals; a `String` assembled at the call
/// site could not promise that. Built by the `words!` table below, so the
/// plural is written once and the list form is concatenated from it at compile
/// time.
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
/// from each other. The list form is neither: it is the plural, and saying so
/// here is what stops the two from drifting.
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
/// Declaring a field *requires* it. A field that may be absent says so with
/// `optional=#true`, because an absent field is exactly the failure the schema
/// exists to catch: a template reading `page.frontmatter.hero` for a page that
/// never set one renders nothing at all, and the build stays green.
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct FieldSchema {
    /// The shape the value must have. [`FieldType::Any`] (the default, and what
    /// a bare `hero` means) constrains only presence.
    pub ty: FieldType,
    /// Whether the page may leave the field out.
    pub optional: bool,
}

/// The shape a schema field requires of a frontmatter value.
///
/// These are the Typst types a page can write in a frontmatter dict: `date` is
/// `datetime(..)`, `dict` a dictionary, and `list<T>` an array whose every
/// element is a `T`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
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
    /// An array whose every element has this type. Bare `list` is `list<str>`,
    /// which is what it meant before the type language had a parameter.
    List(Box<Self>),
    /// A dictionary, and the fields it must carry. Empty (a bare `dict`)
    /// constrains the shape and nothing inside it.
    Dict(Vec<(String, FieldSchema)>),
}

impl FieldType {
    /// The constructor spelling, and the only compound one.
    const LIST: &'static str = "list";

    /// The leaf types, in the order the reference lists them: the single source
    /// of truth for what the innermost name of a type expression may be, read by
    /// both the parser and [`names`](Self::names).
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

    /// Every spelling the type language accepts, for the generated reference and
    /// for the "valid values" help on a typo. A function rather than a constant
    /// because `list<..>` is a shape and not a variant, so the list is the leaf
    /// table plus the two ways to write the constructor over it.
    pub fn names() -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Self::leaves().into_iter().map(|(n, _)| n).collect();
        names.push(Self::LIST);
        names.push("list<..>");
        names
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
            // `list` alone: the parameter it had before it could take one.
            if rest.is_empty() {
                name = "str";
                depth += 1;
                break;
            }
            // Anything else that merely starts with those four letters is a leaf
            // name, and fails as one ("listy" is unknown, not malformed).
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
        if name.is_empty() || name.contains(['<', '>']) {
            return Err(TypeError::Malformed(src.trim().to_owned()));
        }
        Self::leaves()
            .into_iter()
            .find(|(known, _)| *known == name)
            .map(|(_, ty)| ty)
            .ok_or_else(|| TypeError::Unknown(name.to_owned()))
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

    /// The English a diagnostic names this type by: the single table every
    /// message describing a frontmatter value reads, whether it came from a
    /// declared schema or from a built-in key's own reader.
    ///
    /// A list is named after what it holds, so [`article`](Self::article) and
    /// [`plural`](Self::plural) answer for one before reaching here; the row is
    /// the bare form, which is what a `list` with no parameter would be called.
    pub fn words(&self) -> Words {
        match self {
            Self::Any => words!("any value", "values"),
            Self::Str => words!("a string", "strings"),
            Self::Bool => words!("a boolean", "booleans"),
            Self::Int => words!("an integer", "integers"),
            Self::Float => words!("a float", "floats"),
            Self::Date => words!("a date", "dates"),
            Self::Dict(_) => words!("a dictionary", "dictionaries"),
            Self::List(_) => words!("a list", "lists"),
        }
    }

    /// How a diagnostic names this type ("a string"), so the schema errors read
    /// like the built-in frontmatter ones rather than printing a config keyword.
    pub fn article(&self) -> String {
        match self {
            Self::List(inner) => format!("a list of {}", inner.plural()),
            _ => self.words().article.to_owned(),
        }
    }

    /// How [`article`](Self::article) names this type inside a list, so a nested
    /// one reads as "a list of lists of integers" rather than stacking articles.
    fn plural(&self) -> String {
        match self {
            Self::List(inner) => format!("lists of {}", inner.plural()),
            _ => self.words().plural.to_owned(),
        }
    }

    /// A Typst literal of this type, for the "add the field" help. Beside
    /// [`article`](Self::article) because a message that says what is missing
    /// and one that shows how to write it are the same fact. A declared
    /// dictionary shows the fields it requires, since those are the next thing
    /// the author would have got wrong.
    pub fn example(&self) -> String {
        match self {
            Self::Any | Self::Str => "\"..\"".to_owned(),
            Self::Bool => "false".to_owned(),
            Self::Int => "0".to_owned(),
            Self::Float => "0.0".to_owned(),
            Self::Date => "datetime(year: 2024, month: 1, day: 1)".to_owned(),
            Self::List(inner) => format!("({},)", inner.example()),
            Self::Dict(fields) => {
                let required: Vec<String> = fields
                    .iter()
                    .filter(|(_, field)| !field.optional)
                    .map(|(key, field)| format!("{key}: {}", field.ty.example()))
                    .collect();
                match required.is_empty() {
                    true => "(:)".to_owned(),
                    false => format!("({})", required.join(", ")),
                }
            }
        }
    }
}

/// A type expression that names no type.
///
/// Not a diagnostic itself: the config layer owns the span, and an unknown leaf
/// is the same "unknown value" every other named config value raises, listed out
/// of the very table that parses them.
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
    /// One `title "str" optional=#true` line: the node name is the frontmatter
    /// key it constrains, and an optional leading positional its type, so both
    /// the bare `title` (present, any shape) and the terse `tags "list"` read.
    ///
    /// A `{ .. }` block declares the fields of the dictionary the type ends in,
    /// through however many `list<..>` wrap it, and recurses through this same
    /// reader: `authors "list<dict>" { name "str" }`.
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
        // A built-in key's type is fixed by the frontmatter reader, and that
        // reader runs first: it would reject the value before the schema ever
        // saw it. A contradiction is therefore unsatisfiable, and fails at the
        // line that wrote it rather than on every page of the collection.
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

/// One schema field: the shape a frontmatter value must have, and whether the
/// page may leave it out.
impl Attributed for FieldSchema {
    /// The type, written as the leading positional.
    const LEADING: usize = 1;

    /// The one attribute scope with a block of its own: `item` reads it as the
    /// fields of the dictionary the type ends in.
    const NESTS: bool = true;

    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "type",
            Choice(FieldType::names),
            "The shape the value must have, also writable as the leading positional: `title \"str\"`. A list names what it holds: `list<int>`, `list<dict>`.",
            |c, v, t, s| {
                c.ty = v.ty(t, s)?;
                Ok(())
            },
        ),
        (
            "optional",
            Flag,
            "Let the field be absent. Declaring a field otherwise requires it.",
            |c, v, t, s| {
                c.optional = v.boolean(t, s)?;
                Ok(())
            },
        ),
    ]);
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
        // bare `list` keeps the parameter it had before there were parameters
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
    fn rejects_unknown_leaves_and_broken_expressions() {
        assert_eq!(
            FieldType::parse("strr"),
            Err(TypeError::Unknown("strr".to_owned()))
        );
        // four letters of `list` and nothing else is a leaf name, not a list
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

    /// The static list form and the one `article` composes are the same words,
    /// and have to stay so: a frontmatter reader needs a `&'static str` and
    /// reads the first, while a schema mismatch renders the second, and the two
    /// describe the same failure to the same reader.
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
                },
            ),
        ];
        // optional fields are left out: the help shows the smallest value that
        // would satisfy the declaration.
        assert_eq!(list(*inner).example(), "((name: \"..\"),)");
    }
}
