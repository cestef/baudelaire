//! One structured [`Value`] tree, rendered to generated Typst source, to
//! generated JavaScript, or to a Typst runtime value, with every string escaped
//! by its [`Format`].

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
/// [`Format`] or converted to a Typst runtime value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// A sequence: Typst `(a, b)`, JavaScript `[a, b]`.
    Array(Vec<Self>),
    /// A mapping: Typst `(key: value)`, JavaScript `{ "key": value }`. Keys are
    /// quoted where the target requires it, so arbitrary keys are safe.
    Dict(Vec<(String, Self)>),
    /// A pre-formed expression in the *target's* own syntax, emitted verbatim;
    /// it is Typst-only and becomes `null` elsewhere.
    Raw(String),
    None,
}

/// The discriminant is hashed, so `Str("x")` and `Raw("x")` stay apart, and an
/// `f64` by its bit pattern, since two encodings differing in bits are
/// different generated output.
impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Str(s) | Self::Raw(s) => s.hash(state),
            Self::Int(i) => i.hash(state),
            Self::Float(f) => f.to_bits().hash(state),
            Self::Bool(b) => b.hash(state),
            Self::Array(items) => items.hash(state),
            Self::Dict(entries) => entries.hash(state),
            Self::None => {}
        }
    }
}

impl Value {
    pub fn str(value: impl AsRef<str>) -> Self {
        Self::Str(value.as_ref().to_owned())
    }

    /// A string value, or [`Value::None`] for `Option::None`.
    pub fn opt(value: Option<impl Into<String>>) -> Self {
        value.map_or(Self::None, |v| Self::Str(v.into()))
    }

    pub fn array(items: impl IntoIterator<Item = Self>) -> Self {
        Self::Array(items.into_iter().collect())
    }

    pub fn dict<K: Into<String>>(pairs: impl IntoIterator<Item = (K, Self)>) -> Self {
        Self::Dict(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// This dictionary laid over `base`, as Typst spells a merge: a key both
    /// carry is this one's.
    ///
    /// [`Raw`](Self::Raw), because a merge is an expression rather than a
    /// value: what either side holds is only known once the page compiles.
    pub fn over(self, base: &Self) -> Self {
        Self::Raw(format!("{} + {}", Typst(base), Typst(&self)))
    }

    /// The string content, and `None` for any other variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The value under `key`, for a `Dict`, and `None` for any other variant.
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Dict(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Render into `out` in the target language `F`, which the [`Typst`] and
    /// [`Js`] display adapters drive.
    pub fn render<F: Format>(&self, out: &mut String) {
        match self {
            Self::Str(s) => F::string(s, out),
            Self::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Float(n) => F::float(*n, out),
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

/// Only a shape with no counterpart here (content, a function, a length) is
/// carried as its `repr`, since [`Raw`](Value::Raw) renders as `null` in
/// JavaScript and is opaque to [`as_str`](Value::as_str).
impl From<&typst::foundations::Value> for Value {
    fn from(value: &typst::foundations::Value) -> Self {
        use typst::foundations::Value as Typst;
        match value {
            Typst::Str(s) => Self::Str(s.to_string()),
            Typst::Int(n) => Self::Int(*n),
            Typst::Float(n) => Self::Float(*n),
            Typst::Bool(b) => Self::Bool(*b),
            Typst::None => Self::None,
            Typst::Array(items) => Self::Array(items.iter().map(Self::from).collect()),
            Typst::Dict(pairs) => Self::Dict(
                pairs
                    .iter()
                    .map(|(key, value)| (key.to_string(), Self::from(value)))
                    .collect(),
            ),
            other => Self::Raw(other.repr().to_string()),
        }
    }
}

/// A [`Raw`](Value::Raw) expression has no runtime equivalent and becomes
/// `none`.
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
pub trait Format {
    const NONE: &'static str;
    /// The `(open, close)` brackets around a sequence.
    const ARRAY: (&'static str, &'static str);
    /// Emitted after the last element of a non-empty sequence: a trailing `, `
    /// in Typst, so `(x, )` stays an array.
    const ARRAY_TRAILING: &'static str;
    /// The `(open, close)` brackets around a mapping.
    const DICT: (&'static str, &'static str);
    /// The literal for an empty mapping (Typst needs `(:)`, not `()`).
    const EMPTY_DICT: &'static str;

    fn string(s: &str, out: &mut String);
    /// Write a floating-point number, which each language spells its own way
    /// once it is not finite and has to keep its type when it is.
    fn float(n: f64, out: &mut String);
    /// Write a mapping key followed by its `: ` separator.
    fn key(key: &str, out: &mut String);
    /// Write a [`Value::Raw`] expression, which only Typst carries.
    fn raw(_source: &str, out: &mut String) {
        out.push_str("null");
    }
}

/// Generated Typst source: parenthesised arrays/dicts, bare identifier keys.
pub(crate) struct TypstFmt;

impl TypstFmt {
    /// Whether `s` is a name a `#let` can bind and a dict key that can be
    /// written bare.
    ///
    /// Not [`typst::syntax::is_ident`], which is the lexer's character rule and
    /// so admits every keyword; the keyword set is derived by parsing `s` as
    /// code rather than restated here.
    pub(crate) fn bindable(s: &str) -> bool {
        if !typst::syntax::is_ident(s) {
            return false;
        }
        let code = typst::syntax::parse_code(s);
        let mut nodes = code.children();
        match (nodes.next(), nodes.next()) {
            (Some(node), None) => {
                node.kind() == typst::syntax::SyntaxKind::Ident && node.leaf_text() == s
            }
            _ => false,
        }
    }
}

impl Format for TypstFmt {
    const NONE: &'static str = "none";
    const ARRAY: (&'static str, &'static str) = ("(", ")");
    const ARRAY_TRAILING: &'static str = ", ";
    const DICT: (&'static str, &'static str) = ("(", ")");
    const EMPTY_DICT: &'static str = "(:)";

    fn string(s: &str, out: &mut String) {
        let _ = write!(out, "{}", Str(s));
    }

    fn float(n: f64, out: &mut String) {
        if n.is_nan() {
            out.push_str("float.nan");
        } else if n.is_infinite() {
            out.push_str(if n.is_sign_negative() {
                "-float.inf"
            } else {
                "float.inf"
            });
        } else {
            let _ = write!(out, "{n:?}");
        }
    }

    fn key(key: &str, out: &mut String) {
        if Self::bindable(key) {
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

/// A JavaScript expression: bracketed arrays, always-quoted object keys, and
/// strings whose `<`, `>`, `&` and U+2028/U+2029 are escaped, so a value cannot
/// close the `<script>` element it is inlined in or break its parse.
pub(crate) struct JsFmt;

impl JsFmt {
    /// Whether `s` is a plain JavaScript identifier, and so needs no quoting as
    /// an object key or a declared name. Reserved words are *not* excluded,
    /// since they are legal keys.
    pub(crate) fn ident(s: &str) -> bool {
        let mut chars = s.chars();
        let head =
            matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$');
        head && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    }
}

impl Format for JsFmt {
    const NONE: &'static str = "null";
    const ARRAY: (&'static str, &'static str) = ("[", "]");
    const ARRAY_TRAILING: &'static str = "";
    const DICT: (&'static str, &'static str) = ("{", "}");
    const EMPTY_DICT: &'static str = "{}";

    fn string(s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' | '\\' => {
                    out.push('\\');
                    out.push(c);
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c if (c as u32) < 0x20 => {
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }

    fn float(n: f64, out: &mut String) {
        if n.is_nan() {
            out.push_str("NaN");
        } else if n.is_infinite() {
            out.push_str(if n.is_sign_negative() {
                "-Infinity"
            } else {
                "Infinity"
            });
        } else {
            let _ = write!(out, "{n:?}");
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

/// Displays the TypeScript *type* a [`Value`] has: `Ts(&value).to_string()`.
///
/// Read off one sample, so it describes what this build serves rather than
/// every build: a field absent here types as `null`.
pub struct Ts<'a>(pub &'a Value);

impl fmt::Display for Ts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Value::Str(_) => f.write_str("string"),
            Value::Int(_) | Value::Float(_) => f.write_str("number"),
            Value::Bool(_) => f.write_str("boolean"),
            Value::Raw(_) | Value::None => f.write_str("null"),
            Value::Array(items) => match Self::union(items) {
                None => f.write_str("unknown[]"),
                Some(item) => write!(f, "Array<{item}>"),
            },
            Value::Dict(pairs) if pairs.is_empty() => f.write_str("Record<string, never>"),
            Value::Dict(pairs) => {
                f.write_str("{ ")?;
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        f.write_str("; ")?;
                    }
                    if JsFmt::ident(key) {
                        f.write_str(key)?;
                    } else {
                        write!(f, "{}", JsonStr(key))?;
                    }
                    write!(f, ": {}", Self(value))?;
                }
                f.write_str(" }")
            }
        }
    }
}

impl Ts<'_> {
    /// The union of the element types of `items`, deduplicated in first-seen
    /// order, or `None` when there are no elements to read one from.
    fn union<'a>(items: &'a [Value]) -> Option<String> {
        let mut types: Vec<String> = Vec::new();
        for item in items {
            let rendered = Ts::<'a>(item).to_string();
            if !types.contains(&rendered) {
                types.push(rendered);
            }
        }
        (!types.is_empty()).then(|| types.join(" | "))
    }
}

/// Displays a string as a JSON (and so JavaScript) string literal.
struct JsonStr<'a>(&'a str);

impl fmt::Display for JsonStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        JsFmt::string(self.0, &mut out);
        f.write_str(&out)
    }
}

/// Displays serialized JSON as the text of a `<script>` element.
///
/// Every `<` is written as its escape for U+003C, which a JSON parser reads back
/// as the same character and an HTML tokenizer never reads as markup. Every `<`,
/// not just `</`: `<!--<script` puts the tokenizer in the state where
/// the island's own `</script>` no longer closes anything. `<` appears in
/// serialized JSON only inside a string, so replacing it wholesale cannot touch
/// the structure.
pub struct Island<'a>(pub &'a str);

impl fmt::Display for Island<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = self.0.split('<');
        if let Some(first) = parts.next() {
            f.write_str(first)?;
        }
        for part in parts {
            f.write_str("\\u003c")?;
            f.write_str(part)?;
        }
        Ok(())
    }
}

/// Displays a Typst import binding one item under a local alias:
/// `#import "<path>": <item> as <alias>`.
///
/// The path is escaped; the item and the alias are not, and must be Typst
/// identifiers, since an import binds names and no quoting rescues one that is
/// not.
pub struct Import<'a> {
    path: &'a str,
    item: &'a str,
    alias: &'a str,
}

impl<'a> Import<'a> {
    /// `path` is a project-root absolute path (`/templates/post.typ`) or a
    /// package spec; `alias` is `__`-prefixed by every caller, so nothing a
    /// page or a template binds can shadow it.
    pub fn new(path: &'a str, item: &'a str, alias: &'a str) -> Self {
        Self { path, item, alias }
    }
}

impl fmt::Display for Import<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "#import {}: {} as {}",
            Str(self.path),
            self.item,
            self.alias
        )
    }
}

/// Displays a Typst `let` binding: `#let name = <value>`, with the value
/// rendered by [`Typst`].
///
/// The value is escaped; the name is not, and must be a Typst identifier
/// ([`TypstFmt::bindable`]), since a `#let` binds a name rather than a string.
pub struct Let<'a>(pub &'a str, pub &'a Value);

impl fmt::Display for Let<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#let {} = {}", self.0, Typst(self.1))
    }
}

/// A generated Typst call: `#name(a, key: b)`, optionally opening a content
/// block or a sequence of them, with every argument escaped as a [`Value`].
#[must_use]
pub struct Call<'a> {
    name: &'a str,
    args: Vec<(Option<&'static str>, Value)>,
    mode: Mode,
}

/// Whether a generated call is entering Typst from markup or is already in
/// code; getting it wrong is a syntax error in either direction.
#[derive(Clone, Copy)]
enum Mode {
    /// `#name(..)`, the form that opens a call from markup.
    Markup,
    /// `name(..)`, for a call nested inside another one's arguments.
    Code,
}

impl Mode {
    fn sigil(self) -> &'static str {
        match self {
            Self::Markup => "#",
            Self::Code => "",
        }
    }
}

impl<'a> Call<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            args: Vec::new(),
            mode: Mode::Markup,
        }
    }

    /// The same call written in *code* position, where the leading `#` that
    /// enters markup would be a syntax error.
    pub fn bare(mut self) -> Self {
        self.mode = Mode::Code;
        self
    }

    pub fn pos(mut self, value: Value) -> Self {
        self.args.push((None, value));
        self
    }

    /// A named argument, `key: value`.
    ///
    /// The key is `&'static str` because a Typst named argument must be a bare
    /// identifier and no quoting rescues one that is not.
    pub fn named(mut self, key: &'static str, value: Value) -> Self {
        self.args.push((Some(key), value));
        self
    }

    /// The same call wrapping content that is already known: `#name(..)[body]`.
    /// For content that has to stream, see [`Call::content`].
    pub fn body(self, body: impl fmt::Display) -> String {
        format!("{}{body}]", self.content())
    }

    /// The same call opening a content block: `#name(..)[`, which the caller
    /// writes into and closes.
    pub fn content(self) -> Open<'a> {
        Open(self, Bracket::Body)
    }

    /// The same call opening a sequence of content arguments: `#name(..,`. The
    /// caller writes `[..],` per element and closes with `)`.
    pub fn items(self) -> Open<'a> {
        Open(self, Bracket::Items)
    }

    /// The argument list, without the surrounding parentheses.
    fn arguments(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (key, value)) in self.args.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            if let Some(key) = key {
                write!(f, "{key}: ")?;
            }
            write!(f, "{}", Typst(value))?;
        }
        Ok(())
    }
}

impl fmt::Display for Call<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}(", self.mode.sigil(), self.name)?;
        self.arguments(f)?;
        f.write_char(')')
    }
}

/// What an [`Open`] call is about to be handed.
enum Bracket {
    /// One content block: `[` follows, and the caller closes it with `]`.
    Body,
    /// A sequence of them: the argument list stays open and the caller closes
    /// it with `)`.
    Items,
}

/// A call whose content the caller writes itself.
#[must_use]
pub struct Open<'a>(Call<'a>, Bracket);

impl fmt::Display for Open<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.0.mode.sigil(), self.0.name)?;
        match self.1 {
            Bracket::Body if self.0.args.is_empty() => f.write_char('['),
            Bracket::Body => {
                f.write_char('(')?;
                self.0.arguments(f)?;
                f.write_str(")[")
            }
            Bracket::Items => {
                f.write_char('(')?;
                self.0.arguments(f)?;
                if self.0.args.is_empty() {
                    Ok(())
                } else {
                    f.write_str(", ")
                }
            }
        }
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
    use super::{Call, Content, Import, Js, Let, Str, Ts, Typst, Value};

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(Str("a\"b\\c").to_string(), "\"a\\\"b\\\\c\"");
        assert_eq!(Str("plain").to_string(), "\"plain\"");
    }

    /// A page's frontmatter round-trips through this, so a float that arrives
    /// as an int changes what a template computes; `inf` and `nan` are literals
    /// in neither language.
    #[test]
    fn a_float_keeps_its_type_in_both_languages() {
        for (n, typst, js) in [
            (3.0_f64, "3.0", "3.0"),
            (0.5, "0.5", "0.5"),
            (-0.0, "-0.0", "-0.0"),
            (f64::INFINITY, "float.inf", "Infinity"),
            (f64::NEG_INFINITY, "-float.inf", "-Infinity"),
            (f64::NAN, "float.nan", "NaN"),
        ] {
            let value = Value::Float(n);
            assert_eq!(Typst(&value).to_string(), typst, "{n}");
            assert_eq!(Js(&value).to_string(), js, "{n}");
        }
    }

    /// A generated client is inlined into a `<script>` wherever the single-file
    /// export or the router island is on, so no value may close it.
    #[test]
    fn a_js_string_cannot_close_a_script_element() {
        let hostile = Value::str("</script><script>x()</script>\u{2028}&amp;");
        let js = Js(&hostile).to_string();
        assert!(!js.contains('<'), "{js}");
        assert!(!js.contains('>'), "{js}");
        assert!(!js.contains('&'), "{js}");
        assert!(!js.contains('\u{2028}'), "{js}");
        assert_eq!(
            js,
            "\"\\u003c/script\\u003e\\u003cscript\\u003ex()\\u003c/script\\u003e\\u2028\\u0026amp;\""
        );
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
        let v = Value::dict([("a b", Value::Int(1))]);
        assert_eq!(Typst(&v).to_string(), "(\"a b\": 1)");
        assert_eq!(Js(&v).to_string(), "{\"a b\": 1}");
    }

    #[test]
    fn quotes_keys_that_are_typst_keywords() {
        for key in ["in", "as", "let", "set", "show", "context", "none", "auto"] {
            let v = Value::dict([(key, Value::Int(1))]);
            assert_eq!(Typst(&v).to_string(), format!("(\"{key}\": 1)"));
        }
    }

    #[test]
    fn a_bindable_name_is_an_identifier_that_is_not_a_keyword() {
        for name in ["title", "_x", "a-b", "x2", "élan"] {
            assert!(
                super::TypstFmt::bindable(name),
                "{name} is a typst identifier"
            );
        }
        for name in ["in", "none", "true", "a b", "2col", "", "a.b"] {
            assert!(
                !super::TypstFmt::bindable(name),
                "{name} is not one typst can bind"
            );
        }
    }

    #[test]
    fn binds_a_value_as_typst_source() {
        assert_eq!(
            Let("title", &Value::str("A \"B\"")).to_string(),
            "#let title = \"A \\\"B\\\"\""
        );
        assert_eq!(
            Let("langs", &Value::array([Value::str("fr")])).to_string(),
            "#let langs = (\"fr\", )"
        );
    }

    #[test]
    fn imports_bind_one_item_under_an_alias() {
        assert_eq!(
            Import::new("/templates/post.typ", "post", "__layout").to_string(),
            "#import \"/templates/post.typ\": post as __layout"
        );
        assert_eq!(
            Import::new("/a\"b/x.typ", "frontmatter", "__data").to_string(),
            "#import \"/a\\\"b/x.typ\": frontmatter as __data"
        );
    }

    #[test]
    fn round_trips_a_typst_string_value() {
        let typst = typst::foundations::Value::Str("hi".into());
        assert_eq!(Value::from(&typst).as_str(), Some("hi"));
    }

    #[test]
    fn renders_the_type_a_value_has() {
        assert_eq!(
            Ts(&sample()).to_string(),
            "{ title: string; n: number; ok: boolean; items: Array<string>; missing: null; empty: Record<string, never> }"
        );
    }

    #[test]
    fn an_array_types_as_the_union_of_its_members_once_each() {
        let v = Value::array([Value::Int(1), Value::Int(2), Value::str("x")]);
        assert_eq!(Ts(&v).to_string(), "Array<number | string>");
    }

    #[test]
    fn call_arguments_cannot_break_out() {
        let call = Call::new("link")
            .pos(Value::str("/a\")[#sys.exit()]("))
            .to_string();
        assert_eq!(call, r#"#link("/a\")[#sys.exit()](")"#);
    }

    #[test]
    fn a_call_renders_positional_and_named_arguments() {
        let call = Call::new("raw")
            .named("block", Value::Bool(true))
            .named("lang", Value::str("kdl"))
            .pos(Value::str("a b"))
            .to_string();
        assert_eq!(call, r#"#raw(block: true, lang: "kdl", "a b")"#);
    }

    #[test]
    fn an_argumentless_content_call_omits_the_parentheses() {
        assert_eq!(Call::new("emph").content().to_string(), "#emph[");
        assert_eq!(
            Call::new("heading")
                .named("level", Value::Int(2))
                .content()
                .to_string(),
            "#heading(level: 2)["
        );
    }

    #[test]
    fn a_sequence_call_leaves_its_arguments_open() {
        assert_eq!(Call::new("list").items().to_string(), "#list(");
        assert_eq!(
            Call::new("enum")
                .named("start", Value::Int(3))
                .items()
                .to_string(),
            "#enum(start: 3, "
        );
    }

    #[test]
    fn a_raw_argument_is_emitted_verbatim() {
        let call = Call::new("table")
            .named("columns", Value::Int(2))
            .named(
                "align",
                Value::array([Value::Raw("left".into()), Value::Raw("right".into())]),
            )
            .to_string();
        assert_eq!(call, "#table(columns: 2, align: (left, right, ))");

        let positional = Call::new("__layout")
            .pos(Value::Raw("__data".into()))
            .pos(Value::Raw("__body".into()))
            .to_string();
        assert_eq!(positional, "#__layout(__data, __body)");
    }

    #[test]
    fn a_bare_call_drops_the_sigil() {
        assert_eq!(
            Call::new("datetime")
                .named("year", Value::Int(2024))
                .bare()
                .to_string(),
            "datetime(year: 2024)"
        );
        assert_eq!(
            Call::new("table.header").bare().items().to_string(),
            "table.header("
        );
        assert_eq!(Call::new("linebreak").to_string(), "#linebreak()");
    }

    #[test]
    fn a_closed_body_wraps_content_it_is_given() {
        let call = Call::new("link")
            .pos(Value::str("/a"))
            .body(Content("Read \"this\""));
        assert_eq!(call, r#"#link("/a")[#"Read \"this\""]"#);
    }

    #[test]
    fn an_empty_collection_stays_open() {
        assert_eq!(Ts(&Value::array([])).to_string(), "unknown[]");
        assert_eq!(
            Ts(&Value::dict::<&str>([])).to_string(),
            "Record<string, never>"
        );
    }

    #[test]
    fn quotes_a_key_that_is_not_an_identifier() {
        let v = Value::dict([("a b", Value::Int(1))]);
        assert_eq!(Ts(&v).to_string(), "{ \"a b\": number }");
    }
}
