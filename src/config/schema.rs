//! The shapes a collection's frontmatter schema declares, and the small type
//! language that spells them.
//!
//! A field's type is an expression, not a keyword: `list<..>` wraps another
//! type, so `list<int>`, `list<list<int>>` and `list<dict>` all say exactly what
//! they hold. The leaves are the Typst types a page can write in a frontmatter
//! dict, never a second type system. Written beside its own parser and error the
//! way [`Permalink`](crate::config::Permalink) is, because it is the same kind
//! of thing: a mini-language inside a config string.

use crate::config::dispatch::Keys;
use crate::error::{BaudelaireErrorKind, ConfigError, ConfigErrorKind};
use crate::ui::markup;
use miette::SourceSpan;

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

    /// How a diagnostic names this type ("a string"), so the schema errors read
    /// like the built-in frontmatter ones rather than printing a config keyword.
    pub fn article(&self) -> String {
        match self {
            Self::Any => "any value".to_owned(),
            Self::Str => "a string".to_owned(),
            Self::Bool => "a boolean".to_owned(),
            Self::Int => "an integer".to_owned(),
            Self::Float => "a float".to_owned(),
            Self::Date => "a date".to_owned(),
            Self::Dict(_) => "a dictionary".to_owned(),
            Self::List(inner) => format!("a list of {}", inner.plural()),
        }
    }

    /// How [`article`](Self::article) names this type inside a list, so a nested
    /// one reads as "a list of lists of integers" rather than stacking articles.
    fn plural(&self) -> String {
        match self {
            Self::Any => "values".to_owned(),
            Self::Str => "strings".to_owned(),
            Self::Bool => "booleans".to_owned(),
            Self::Int => "integers".to_owned(),
            Self::Float => "floats".to_owned(),
            Self::Date => "dates".to_owned(),
            Self::Dict(_) => "dictionaries".to_owned(),
            Self::List(inner) => format!("lists of {}", inner.plural()),
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
