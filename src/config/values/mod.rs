//! What a config holds, read back out of it: one typed value per key, keyed as
//! the dispatch tables key it.

pub mod source;

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use kdl::KdlValue;

use super::Named;
use crate::ui::Bytes;

/// One key's effective value, after every layer that had something to say.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A key holding nothing: an `Option` field that no layer filled.
    Unset,
    /// A key holding the null a site wrote: `#null`, which only the free
    /// tables a site keys itself can carry.
    Null,
    Flag(bool),
    Number(i64),
    Decimal(f64),
    Text(String),
    /// Several values on one line: `widths 480 960`.
    List(Vec<Self>),
    /// A node, as its own line spells it: the arguments it carries, the
    /// `key=value` attributes on that line, and the keys of the block beneath
    /// it. A section fills `keys`, an attributed line fills `attrs`.
    Node {
        args: Vec<Self>,
        attrs: Vec<(String, Self)>,
        keys: Vec<(String, Self)>,
    },
}

impl Value {
    /// A block of keys, which is what a section holds.
    pub fn block(keys: Vec<(String, Self)>) -> Self {
        Self::Node {
            args: Vec::new(),
            attrs: Vec::new(),
            keys,
        }
    }

    /// One line: the arguments it carries and the attributes written on it,
    /// which is what an attributed item holds.
    pub fn line(args: Vec<Self>, attrs: Vec<(String, Self)>) -> Self {
        Self::Node {
            args,
            attrs,
            keys: Vec::new(),
        }
    }

    /// A line that also opens a block, which only a schema field does.
    pub fn nested(args: Vec<Self>, attrs: Vec<(String, Self)>, keys: Vec<(String, Self)>) -> Self {
        Self::Node { args, attrs, keys }
    }

    /// The same node with `args` written on its own line: how a section that is
    /// switched off, or an item whose value is a positional, spells itself.
    pub fn with(self, args: Vec<Self>) -> Self {
        match self {
            Self::Node { attrs, keys, .. } => Self::Node { args, attrs, keys },
            held => held,
        }
    }

    /// A variant under the name its config table spells it with.
    pub fn named(variant: impl Named) -> Self {
        Self::Text(variant.name().to_owned())
    }

    /// Anything whose `Display` *is* its config spelling: a permalink template,
    /// a schema field's type, a browser version.
    pub fn written(value: impl fmt::Display) -> Self {
        Self::Text(value.to_string())
    }

    /// One entry per name the author chose, each read by its own table.
    pub(crate) fn each<T>(items: &[(String, T)], read: impl Fn(&T) -> Self) -> Self {
        Self::block(
            items
                .iter()
                .map(|(name, item)| (name.clone(), read(item)))
                .collect(),
        )
    }

    /// The value at a dotted path below this one, or `None` where no key by
    /// that name is held.
    pub fn at(&self, key: &str) -> Option<&Self> {
        key.split('.').try_fold(self, |value, step| value.key(step))
    }

    /// One step down: an attribute or a key of the block by that name.
    fn key(&self, key: &str) -> Option<&Self> {
        let Self::Node { attrs, keys, .. } = self else {
            return None;
        };
        attrs
            .iter()
            .chain(keys)
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// What a script reads: one line per value, a block as its keys' own lines,
    /// and a string without the quotes it is written with.
    pub fn scalar(&self) -> String {
        match self {
            Self::Unset => String::new(),
            Self::Flag(on) => on.to_string(),
            Self::Number(n) => n.to_string(),
            Self::Text(text) => text.clone(),
            Self::List(values) => values
                .iter()
                .map(Self::scalar)
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Null => "null".to_owned(),
            Self::Decimal(n) => n.to_string(),
            Self::Node { .. } => Tree::of(self).to_string(),
        }
    }

    /// Whether this value holds nothing at all, which is what an absent
    /// `Option` and an empty repeated block both read as.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Unset => true,
            Self::List(values) => values.is_empty(),
            Self::Node { args, attrs, keys } => {
                args.is_empty() && attrs.is_empty() && keys.is_empty()
            }
            _ => false,
        }
    }
}

/// A value as its config line spells it: what `explain` and `show` print, and
/// what the KDL highlighter is handed.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => Ok(()),
            Self::Flag(on) => write!(f, "#{on}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(text) => f.write_str(&Quoted::of(text)),
            Self::List(values) => {
                let written: Vec<String> = values.iter().map(Self::to_string).collect();
                f.write_str(&written.join(" "))
            }
            Self::Null => f.write_str("#null"),
            Self::Decimal(n) => write!(f, "{n}"),
            Self::Node { .. } => Tree::of(self).fmt(f),
        }
    }
}

/// A block written out as the KDL it would be authored as, one key per line.
pub struct Tree<'a> {
    value: &'a Value,
    depth: usize,
}

impl<'a> Tree<'a> {
    pub fn of(value: &'a Value) -> Self {
        Self { value, depth: 0 }
    }

    fn below(&self, value: &'a Value) -> Self {
        Self {
            value,
            depth: self.depth + 1,
        }
    }
}

/// Written at depth zero the node carries its own line, which at any greater
/// depth its key has already written.
impl fmt::Display for Tree<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Value::Node { args, attrs, keys } = self.value else {
            return self.value.fmt(f);
        };
        if self.depth == 0 && !(args.is_empty() && attrs.is_empty()) {
            let mut line = String::new();
            Self::line(&mut line, args, attrs)?;
            writeln!(f, "{}", line.trim_start())?;
        }
        for (key, value) in keys {
            self.entry(f, key, value)?;
        }
        Ok(())
    }
}

impl Tree<'_> {
    /// One key of a block, written as the node it would be authored as. A key
    /// holding nothing is left out: what is printed has to parse, and a key
    /// written with no value is what several of them refuse.
    fn entry(&self, f: &mut fmt::Formatter<'_>, key: &str, value: &Value) -> fmt::Result {
        let indent = "  ".repeat(self.depth);
        let Value::Node { args, attrs, keys } = value else {
            if value.is_empty() {
                return Ok(());
            }
            return writeln!(f, "{indent}{} {value}", Key(key));
        };
        let inner = self.below(value).to_string();
        if inner.is_empty() && args.is_empty() && attrs.is_empty() {
            return Ok(());
        }
        let _ = keys;
        write!(f, "{indent}{}", Key(key))?;
        Self::line(f, args, attrs)?;
        if inner.is_empty() {
            return writeln!(f);
        }
        write!(f, " {{\n{inner}{indent}}}\n")
    }

    /// The arguments and attributes a node carries on its own line.
    fn line(f: &mut impl fmt::Write, args: &[Value], attrs: &[(String, Value)]) -> fmt::Result {
        for arg in args {
            write!(f, " {arg}")?;
        }
        for (key, value) in attrs {
            if value.is_empty() {
                continue;
            }
            write!(f, " {}={value}", Key(key))?;
        }
        Ok(())
    }
}

/// A string as a config line writes it. Always quoted, KDL's bare form being
/// legal but unread by a highlighter as the string it is.
struct Quoted;

impl Quoted {
    fn of(text: &str) -> String {
        let written = KdlValue::String(text.to_owned()).to_string();
        if written.starts_with('"') {
            written
        } else {
            format!("\"{written}\"")
        }
    }
}

/// A block's key, written as KDL spells it: bare where the parser reads it back
/// as that very name, quoted otherwise.
struct Key<'a>(&'a str);

impl fmt::Display for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", KdlValue::String(self.0.to_owned()))
    }
}

impl From<bool> for Value {
    fn from(on: bool) -> Self {
        Self::Flag(on)
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<&Path> for Value {
    fn from(path: &Path) -> Self {
        Self::Text(path.display().to_string())
    }
}

impl From<PathBuf> for Value {
    fn from(path: PathBuf) -> Self {
        Self::Text(path.display().to_string())
    }
}

/// A size in bytes, whatever unit it was written in.
impl From<Bytes> for Value {
    fn from(size: Bytes) -> Self {
        Self::Number(i64::try_from(size.0).unwrap_or(i64::MAX))
    }
}

/// A length of time in seconds, whatever unit it was written in.
impl From<Duration> for Value {
    fn from(duration: Duration) -> Self {
        Self::Number(i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
    }
}

impl<T: Into<Self>> From<Vec<T>> for Value {
    fn from(values: Vec<T>) -> Self {
        values.into_iter().collect()
    }
}

/// A value a site hands to generated code, read back as the config value it
/// was written as.
impl From<&crate::codegen::Value> for Value {
    fn from(value: &crate::codegen::Value) -> Self {
        use crate::codegen::Value as Generated;
        match value {
            Generated::Str(text) | Generated::Raw(text) => Self::Text(text.clone()),
            Generated::Int(n) => Self::Number(*n),
            Generated::Float(n) => Self::Decimal(*n),
            Generated::Bool(on) => Self::Flag(*on),
            Generated::Array(values) => values.iter().map(Self::from).collect(),
            Generated::Dict(entries) => Self::block(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from(value)))
                    .collect(),
            ),
            Generated::None => Self::Null,
        }
    }
}

impl<T: Into<Self>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Unset, Into::into)
    }
}

impl<T: Into<Self>> FromIterator<T> for Value {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self::List(values.into_iter().map(Into::into).collect())
    }
}

macro_rules! numbers {
    ($($ty:ty),*) => {
        $(impl From<$ty> for Value {
            fn from(n: $ty) -> Self {
                Self::Number(i64::from(n))
            }
        })*
    };
}

numbers!(u8, u16, u32, i8, i16, i32);

impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Self::Number(i64::try_from(n).unwrap_or(i64::MAX))
    }
}

impl From<u64> for Value {
    fn from(n: u64) -> Self {
        Self::Number(i64::try_from(n).unwrap_or(i64::MAX))
    }
}
