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
    Array(Vec<Self>),
    /// A mapping: Typst `(key: value)`, JavaScript `{ "key": value }`. Keys are
    /// quoted where the target requires it, so arbitrary keys are safe.
    Dict(Vec<(String, Self)>),
    /// A pre-formed expression in the *target's* own syntax, emitted verbatim.
    /// Carries a Typst runtime value's [`repr`](typst::foundations::Value::repr)
    /// unchanged; it is Typst-only and becomes `null` / `none` elsewhere.
    Raw(String),
    None,
}

/// A real `Hash`, so nothing has to take a serialization as identity to
/// fingerprint a value (the cache key of every config-injected value and every
/// tracked `sys.inputs` read goes through here). The match destructures every
/// variant, so a new one fails to compile until it is handled; the discriminant
/// keeps `Str("x")` and `Raw("x")` apart. `f64` hashes by bit pattern: `Value`
/// is only `PartialEq`, and two encodings that differ in bits are different
/// generated output.
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

    /// The string content, for a `Str` value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The value under `key`, for a `Dict`, so a consumer can serve a sub-tree
    /// of a larger value (a JS module exposing part of the build context).
    pub fn get(&self, key: &str) -> Option<&Self> {
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

/// Read a Typst runtime value into a [`Value`], mapping every shape that has a
/// counterpart here and carrying the rest as its `repr`.
///
/// [`Raw`](Value::Raw) is Typst-only by construction, so a value that reaches it
/// renders as `null` in JavaScript and is opaque to
/// [`as_str`](Value::as_str). Everything but `Str` used to land there, which
/// made a frontmatter `weight: 3` arrive in the browser as `null` while a config
/// `client { retries 3 }` arrived as `3`: one type, two behaviours, decided by
/// which parser filled it. What is left for `Raw` is what genuinely has no
/// counterpart -- content, a function, a length.
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

/// Displays the TypeScript *type* a [`Value`] has: `Ts(&value).to_string()`.
///
/// The odd one out among the adapters, which render a value *as* source in a
/// target language. This renders what a declaration file has to say about it,
/// so a generated module's data can be typed from the very tree the module is
/// built from instead of from a hand-written shape that drifts from it.
///
/// Types come from one sample, so it describes what this build serves, not
/// every build: a string field is `string`, and one that happens to be absent
/// here is `null`.
pub struct Ts<'a>(pub &'a Value);

impl fmt::Display for Ts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Value::Str(_) => f.write_str("string"),
            Value::Int(_) | Value::Float(_) => f.write_str("number"),
            Value::Bool(_) => f.write_str("boolean"),
            // A Typst-only expression reaches JavaScript as `null`, exactly as
            // an absent value does, so the two type the same.
            Value::Raw(_) | Value::None => f.write_str("null"),
            Value::Array(items) => match Self::union(items) {
                // An empty array constrains nothing, and `never[]` would refuse
                // every element a later build puts in it.
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
                    match ident(key) {
                        true => f.write_str(key)?,
                        false => write!(f, "{}", JsonStr(key))?,
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
    ///
    /// Deduplicated because rows of one shape are the common case, and a
    /// hundred identical members would otherwise be spelled a hundred times.
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

/// Whether `s` is a plain JavaScript identifier, and so needs no quoting as an
/// object key or a declared name. Reserved words are *not* excluded: they are
/// legal keys, and only the callers that emit bindings care.
pub(crate) fn ident(s: &str) -> bool {
    let mut chars = s.chars();
    let head = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$');
    head && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Displays a Typst `let` binding: `#let name = <value>`, with the value
/// rendered by [`Typst`]. The one way generated module source binds a value, so
/// no caller ever formats a binding (and its escaping) by hand.
pub struct Let<'a>(pub &'a str, pub &'a Value);

impl fmt::Display for Let<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#let {} = {}", self.0, Typst(self.1))
    }
}

/// A generated Typst call: `#name(a, key: b)`, optionally opening a content
/// block or a sequence of them.
///
/// Every argument is a [`Value`], so it is escaped by the one renderer that
/// knows how, and no caller assembles `#name(` and its commas by hand. That
/// matters most where the generated source is built from user input: a call
/// spelled with `format!` is escaped only as carefully as its least careful
/// site, and there is no compiler check that says so.
#[must_use]
pub struct Call<'a> {
    name: &'a str,
    args: Vec<(Option<&'static str>, Value)>,
}

impl<'a> Call<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            args: Vec::new(),
        }
    }

    /// A positional argument.
    pub fn pos(mut self, value: Value) -> Self {
        self.args.push((None, value));
        self
    }

    /// A named argument, `key: value`.
    ///
    /// The key is `&'static str` on purpose. A Typst named argument must be a
    /// bare identifier -- `#raw("a b": 1)` is refused as firmly as
    /// `#raw(a b: 1)` -- so unlike [`Format::key`], which can quote its way out,
    /// there is no rendering that rescues a bad one. A literal is the only
    /// thing that can be checked by reading the call, so it is the only thing
    /// accepted.
    pub fn named(mut self, key: &'static str, value: Value) -> Self {
        self.args.push((Some(key), value));
        self
    }

    /// The same call wrapping content that is already known: `#name(..)[body]`.
    /// For content that has to stream, see [`Call::content`].
    pub fn body(self, body: impl fmt::Display) -> String {
        format!("{}{body}]", self.content())
    }

    /// The same call opening a content block: `#name(..)[`. The caller writes
    /// the content and closes it, which is what lets content stream rather than
    /// be buffered whole.
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
        write!(f, "#{}(", self.name)?;
        self.arguments(f)?;
        f.write_char(')')
    }
}

/// What an [`Open`] call is about to be handed. Not spelled `Content`: that
/// name belongs to the display adapter below, and means something else.
enum Bracket {
    /// One content block: `[` follows, and the caller closes it with `]`.
    Body,
    /// A sequence of them: the argument list stays open and the caller closes
    /// it with `)`.
    Items,
}

/// A call whose content the caller writes itself. See [`Call::content`] and
/// [`Call::items`].
#[must_use]
pub struct Open<'a>(Call<'a>, Bracket);

impl fmt::Display for Open<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0.name)?;
        match self.1 {
            // `#emph[` rather than `#emph()[`: an empty argument list is legal
            // but reads as a call that forgot something.
            Bracket::Body if self.0.args.is_empty() => f.write_char('['),
            Bracket::Body => {
                f.write_char('(')?;
                self.0.arguments(f)?;
                f.write_str(")[")
            }
            Bracket::Items => {
                f.write_char('(')?;
                self.0.arguments(f)?;
                match self.0.args.is_empty() {
                    true => Ok(()),
                    false => f.write_str(", "),
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
    use super::{Call, Content, Js, Let, Str, Ts, Typst, Value};

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

    /// Rows of one shape are the common case, and repeating a member type once
    /// per element would make a catalogue's type unreadable.
    #[test]
    fn an_array_types_as_the_union_of_its_members_once_each() {
        let v = Value::array([Value::Int(1), Value::Int(2), Value::str("x")]);
        assert_eq!(Ts(&v).to_string(), "Array<number | string>");
    }

    /// Every argument is escaped by the renderer, so a call can carry a value
    /// that would otherwise close it and start writing Typst.
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

    /// `#emph[` rather than `#emph()[`: an argumentless call opening a content
    /// block should read the way an author would write it.
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

    /// A sequence leaves the argument list open for the caller's `[..],`
    /// elements, and closes the preceding arguments with a comma so the first
    /// element does not fuse onto them.
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

    /// `Value::Raw` is the one unescaped door, and it is the same one the rest
    /// of this module uses: identifiers no string can spell reach a call
    /// through it, positionally as well as by name.
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

    /// A call whose content is already in hand closes itself, so a caller with
    /// nothing to stream does not assemble the brackets by hand.
    #[test]
    fn a_closed_body_wraps_content_it_is_given() {
        let call = Call::new("link")
            .pos(Value::str("/a"))
            .body(Content("Read \"this\""));
        assert_eq!(call, r#"#link("/a")[#"Read \"this\""]"#);
    }

    /// An empty array constrains nothing: `never[]` would refuse every element
    /// the next build puts in it.
    #[test]
    fn an_empty_collection_stays_open() {
        assert_eq!(Ts(&Value::array([])).to_string(), "unknown[]");
        assert_eq!(
            Ts(&Value::dict::<&str>([])).to_string(),
            "Record<string, never>"
        );
    }

    /// A key that is not an identifier has to be quoted, or the type does not
    /// parse; the same rule the JS renderer applies to every key.
    #[test]
    fn quotes_a_key_that_is_not_an_identifier() {
        let v = Value::dict([("a b", Value::Int(1))]);
        assert_eq!(Ts(&v).to_string(), "{ \"a b\": number }");
    }
}
