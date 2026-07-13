//! Helpers for generating typst source safely.
//!
//! Baudelaire builds synthetic typst (layout binding, taxonomy pages) rather
//! than templating HTML strings. Any user-derived value spliced into that
//! source must be a properly escaped typst *string literal* so it can't break
//! out of the literal or inject markup.

use std::fmt::{self, Write};

use serde::{Deserialize, Serialize};
use typst::foundations::Repr;

/// Displays a string as a typst string literal, escaping `"` and `\`.
pub struct Str<'a>(pub &'a str);

impl fmt::Display for Str<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        for c in self.0.chars() {
            match c {
                '"' | '\\' => {
                    f.write_char('\\')?;
                    f.write_char(c)?;
                }
                _ => f.write_char(c)?,
            }
        }
        f.write_char('"')
    }
}

/// A typst value built in Rust and serialized to a literal via [`fmt::Display`].
///
/// This is the safe way to pass structured data into generated typst: build a
/// `Value` tree and render it once, so every string is escaped by the single
/// [`Str`] path and there is no ad-hoc `format!` per data shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    /// A typst array `(a, b, …)`.
    Array(Vec<Value>),
    /// A typst dictionary `(key: value, …)`. Non-identifier keys are quoted, so
    /// arbitrary frontmatter keys are safe.
    Dict(Vec<(String, Value)>),
    /// A pre-formed, already-valid typst expression, emitted verbatim. Used to
    /// carry a typst runtime value's own [`repr`](typst::foundations::Value::repr)
    /// through unchanged.
    Raw(String),
    None,
}

impl Value {
    pub fn str(value: impl AsRef<str>) -> Self {
        Self::Str(value.as_ref().to_owned())
    }

    /// A string value, or [`Value::None`] for `Option::None`.
    pub fn opt(value: Option<impl Into<String>>) -> Self {
        value.map_or(Self::None, |v| Self::Str(v.into()))
    }

    pub fn array(items: impl IntoIterator<Item = Value>) -> Self {
        Self::Array(items.into_iter().collect())
    }

    pub fn dict<K: Into<String>>(pairs: impl IntoIterator<Item = (K, Value)>) -> Self {
        Self::Dict(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Carry a typst runtime value into generated source via its own `repr`,
    /// which is valid typst for data values (strings, numbers, arrays, dicts, …).
    pub fn from_typst(value: &typst::foundations::Value) -> Self {
        Self::Raw(value.repr().to_string())
    }

    /// Store a typst data value with its string content preserved for strings
    /// (so it round-trips back to readable text via [`Value::as_str`]) and its
    /// `repr` for everything else. Renders to the same typst source either way —
    /// a `Str` is quoted like the string's `repr` — but keeps string frontmatter
    /// readable rather than opaque source.
    pub fn from_typst_data(value: &typst::foundations::Value) -> Self {
        match value {
            typst::foundations::Value::Str(s) => Self::Str(s.to_string()),
            other => Self::from_typst(other),
        }
    }

    /// The string content, for a `Str` value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(s) => Str(s).fmt(f),
            Self::Int(n) => write!(f, "{n}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::None => f.write_str("none"),
            Self::Array(items) => {
                f.write_char('(')?;
                for item in items {
                    // Trailing comma keeps a one-element `(x,)` an array, not a group.
                    write!(f, "{item}, ")?;
                }
                f.write_char(')')
            }
            Self::Dict(pairs) if pairs.is_empty() => f.write_str("(:)"),
            Self::Dict(pairs) => {
                f.write_char('(')?;
                for (i, (key, value)) in pairs.iter().enumerate() {
                    let sep = if i == 0 { "" } else { ", " };
                    if typst::syntax::is_ident(key) {
                        write!(f, "{sep}{key}: {value}")?;
                    } else {
                        write!(f, "{sep}{}: {value}", Str(key))?;
                    }
                }
                f.write_char(')')
            }
            Self::Raw(source) => f.write_str(source),
        }
    }
}

/// Displays a string as typst *content* that renders literally — `#"..."` — so
/// user text can never inject markup.
pub struct Content<'a>(pub &'a str);

impl fmt::Display for Content<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", Str(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::Str;

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(Str("a\"b\\c").to_string(), "\"a\\\"b\\\\c\"");
        assert_eq!(Str("plain").to_string(), "\"plain\"");
    }

    #[test]
    fn value_serializes_to_valid_typst() {
        use super::Value;
        let v = Value::dict([
            ("title", Value::str("A \"B\"")),
            ("n", Value::Int(3)),
            ("ok", Value::Bool(true)),
            ("items", Value::array([Value::str("x")])),
            ("missing", Value::opt(None::<String>)),
            ("empty", Value::dict::<&str>([])),
        ]);
        assert_eq!(
            v.to_string(),
            "(title: \"A \\\"B\\\"\", n: 3, ok: true, items: (\"x\", ), missing: none, empty: (:))"
        );
    }
}
