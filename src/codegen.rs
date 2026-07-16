//! One structured value, rendered to several targets.
//!
//! Baudelaire moves data across three boundaries: into generated **Typst**
//! source (layout binding, taxonomy pages, `page.sections`), into generated
//! **JavaScript** (the `baudelaire:*` virtual modules), and into a Typst
//! **runtime** value (`sys.inputs`). All start from one [`Value`] tree:
//!
//! - source in a target language -> wrap in the [`Typst`] or [`Js`] display
//!   adapter (`Typst(&value).to_string()`), which name the target explicitly
//!   rather than overloading `Display` on the data;
//! - a runtime Typst value -> `typst::foundations::Value::from(&value)`;
//! - a Typst value read back -> `Value::from(&typst_value)`.
//!
//! Every string is escaped by its [`Format`], so a value can neither break out
//! of a Typst string literal nor produce invalid JavaScript. Add a target by
//! implementing `Format` and pairing it with a one-line display adapter.

use std::fmt::{self, Write};

use serde::{Deserialize, Serialize};
use typst::foundations::Repr;

/// Displays a string as a Typst string literal, escaping `"` and `\`: the only
/// two metacharacters inside a Typst quoted string.
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

/// A structured value built in Rust, rendered to a target language through a
/// [`Format`] (via the [`Typst`] / [`Js`] adapters) or converted to a Typst
/// runtime value. The single safe way to move data into generated code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// A sequence: Typst `(a, b)`, JavaScript `[a, b]`.
    Array(Vec<Value>),
    /// A mapping: Typst `(key: value)`, JavaScript `{ "key": value }`. Keys are
    /// quoted where the target requires it, so arbitrary keys are safe.
    Dict(Vec<(String, Value)>),
    /// A pre-formed expression in the *target's* own syntax, emitted verbatim.
    /// Carries a Typst runtime value's [`repr`](typst::foundations::Value::repr)
    /// unchanged; it is Typst-only and becomes `null` / `none` elsewhere.
    Raw(String),
    None,
}

impl Value {
    pub fn str(value: impl AsRef<str>) -> Self {
        Self::Str(value.as_ref().to_owned())
    }

    pub fn float(value: f64) -> Self {
        Self::Float(value)
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

    /// The string content, for a `Str` value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The value under `key`, for a `Dict`, so a consumer can serve a sub-tree
    /// of a larger value (a JS module exposing part of the build context).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Dict(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Render into `out` in the target language `F`. The [`Typst`] and [`Js`]
    /// display adapters drive this; call it directly only to add a new target.
    pub fn render<F: Format>(&self, out: &mut String) {
        match self {
            Self::Str(s) => F::string(s, out),
            Self::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Float(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Bool(b) => {
                let _ = write!(out, "{b}");
            }
            Self::None => out.push_str(F::NONE),
            Self::Raw(source) => F::raw(source, out),
            Self::Array(items) => {
                out.push_str(F::ARRAY.0);
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.render::<F>(out);
                }
                if !items.is_empty() {
                    out.push_str(F::ARRAY_TRAILING);
                }
                out.push_str(F::ARRAY.1);
            }
            Self::Dict(pairs) if pairs.is_empty() => out.push_str(F::EMPTY_DICT),
            Self::Dict(pairs) => {
                out.push_str(F::DICT.0);
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    F::key(key, out);
                    value.render::<F>(out);
                }
                out.push_str(F::DICT.1);
            }
        }
    }
}

/// Read a Typst runtime value into a [`Value`], keeping a string's content
/// readable (so it round-trips through [`as_str`](Value::as_str)) and carrying
/// everything else as its `repr`.
impl From<&typst::foundations::Value> for Value {
    fn from(value: &typst::foundations::Value) -> Self {
        match value {
            typst::foundations::Value::Str(s) => Self::Str(s.to_string()),
            other => Self::Raw(other.repr().to_string()),
        }
    }
}

/// A [`Value`] as a Typst runtime value, for injection into `sys.inputs`. A
/// [`Raw`](Value::Raw) expression has no runtime equivalent and becomes `none`.
impl From<&Value> for typst::foundations::Value {
    fn from(value: &Value) -> Self {
        use typst::foundations::{Array, Dict, IntoValue, Str as TypstStr};
        match value {
            Value::Str(s) => s.clone().into_value(),
            Value::Int(n) => (*n).into_value(),
            Value::Float(n) => (*n).into_value(),
            Value::Bool(b) => (*b).into_value(),
            Value::None | Value::Raw(_) => Self::None,
            Value::Array(items) => items.iter().map(Self::from).collect::<Array>().into_value(),
            Value::Dict(pairs) => pairs
                .iter()
                .map(|(key, value)| (TypstStr::from(key.as_str()), Self::from(value)))
                .collect::<Dict>()
                .into_value(),
        }
    }
}

/// A target language a [`Value`] renders to: the brackets around sequences and
/// mappings, and how it writes a string, a key, and a verbatim expression.
/// Numbers and booleans render the same everywhere, so a format is a few
/// constants and three small methods.
pub trait Format {
    /// The literal for [`Value::None`].
    const NONE: &'static str;
    /// The `(open, close)` brackets around a sequence.
    const ARRAY: (&'static str, &'static str);
    /// Emitted after the last element of a non-empty sequence: a trailing `, `
    /// in Typst (so `(x, )` stays an array), nothing in JavaScript.
    const ARRAY_TRAILING: &'static str;
    /// The `(open, close)` brackets around a mapping.
    const DICT: (&'static str, &'static str);
    /// The literal for an empty mapping (Typst needs `(:)`, not `()`).
    const EMPTY_DICT: &'static str;

    /// Write a string literal.
    fn string(s: &str, out: &mut String);
    /// Write a mapping key followed by its `: ` separator.
    fn key(key: &str, out: &mut String);
    /// Write a [`Value::Raw`] expression. Only Typst carries these; the default
    /// renders `null` for every other target.
    fn raw(_source: &str, out: &mut String) {
        out.push_str("null");
    }
}

/// Generated Typst source: parenthesised arrays/dicts, bare identifier keys.
struct TypstFmt;

impl Format for TypstFmt {
    const NONE: &'static str = "none";
    const ARRAY: (&'static str, &'static str) = ("(", ")");
    const ARRAY_TRAILING: &'static str = ", ";
    const DICT: (&'static str, &'static str) = ("(", ")");
    const EMPTY_DICT: &'static str = "(:)";

    fn string(s: &str, out: &mut String) {
        let _ = write!(out, "{}", Str(s));
    }

    fn key(key: &str, out: &mut String) {
        if typst::syntax::is_ident(key) {
            out.push_str(key);
        } else {
            let _ = write!(out, "{}", Str(key));
        }
        out.push_str(": ");
    }

    fn raw(source: &str, out: &mut String) {
        out.push_str(source);
    }
}

/// A JavaScript expression: bracketed arrays, always-quoted object keys, strings
/// escaped by `serde_json` (the JSON string grammar is a strict JS subset).
struct JsFmt;

impl Format for JsFmt {
    const NONE: &'static str = "null";
    const ARRAY: (&'static str, &'static str) = ("[", "]");
    const ARRAY_TRAILING: &'static str = "";
    const DICT: (&'static str, &'static str) = ("{", "}");
    const EMPTY_DICT: &'static str = "{}";

    fn string(s: &str, out: &mut String) {
        match serde_json::to_string(s) {
            Ok(escaped) => out.push_str(&escaped),
            Err(_) => out.push_str("\"\""),
        }
    }

    fn key(key: &str, out: &mut String) {
        Self::string(key, out);
        out.push_str(": ");
    }
}

/// Displays a [`Value`] as Typst source: `Typst(&value).to_string()`.
pub struct Typst<'a>(pub &'a Value);

impl fmt::Display for Typst<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.0.render::<TypstFmt>(&mut out);
        f.write_str(&out)
    }
}

/// Displays a [`Value`] as a JavaScript expression: `Js(&value).to_string()`.
pub struct Js<'a>(pub &'a Value);

impl fmt::Display for Js<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.0.render::<JsFmt>(&mut out);
        f.write_str(&out)
    }
}

/// Displays a string as Typst *content* that renders literally (`#"..."`), so
/// user text can never inject markup.
pub struct Content<'a>(pub &'a str);

impl fmt::Display for Content<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", Str(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::{Js, Str, Typst, Value};

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(Str("a\"b\\c").to_string(), "\"a\\\"b\\\\c\"");
        assert_eq!(Str("plain").to_string(), "\"plain\"");
    }

    fn sample() -> Value {
        Value::dict([
            ("title", Value::str("A \"B\"")),
            ("n", Value::Int(3)),
            ("ok", Value::Bool(true)),
            ("items", Value::array([Value::str("x")])),
            ("missing", Value::opt(None::<String>)),
            ("empty", Value::dict::<&str>([])),
        ])
    }

    #[test]
    fn renders_valid_typst() {
        assert_eq!(
            Typst(&sample()).to_string(),
            "(title: \"A \\\"B\\\"\", n: 3, ok: true, items: (\"x\", ), missing: none, empty: (:))"
        );
    }

    #[test]
    fn renders_valid_javascript() {
        assert_eq!(
            Js(&sample()).to_string(),
            "{\"title\": \"A \\\"B\\\"\", \"n\": 3, \"ok\": true, \"items\": [\"x\"], \"missing\": null, \"empty\": {}}"
        );
    }

    #[test]
    fn quotes_non_identifier_keys_per_target() {
        // A space is not a valid Typst identifier (a hyphen is), so Typst quotes
        // it; JavaScript quotes every key.
        let v = Value::dict([("a b", Value::Int(1))]);
        assert_eq!(Typst(&v).to_string(), "(\"a b\": 1)");
        assert_eq!(Js(&v).to_string(), "{\"a b\": 1}");
    }

    #[test]
    fn round_trips_a_typst_string_value() {
        let typst = typst::foundations::Value::Str("hi".into());
        assert_eq!(Value::from(&typst).as_str(), Some("hi"));
    }
}
