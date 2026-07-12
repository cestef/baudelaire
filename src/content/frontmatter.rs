use std::collections::BTreeMap;
use std::path::Path;

use typst::foundations::{Datetime, Dict, Value};
use typst::syntax::{
    LinkedNode, Source,
    ast::{AstNode, Expr, Markup},
};

use crate::config::Config;
use crate::config::dispatch::Keys;
use crate::error::{ContentError, Result};

/// The recognized scalar/list frontmatter keys (taxonomy keys are configured,
/// so they are added dynamically). The single source both the `from_dict` match
/// and the typo suggester read — a new key here plus a match arm, or the sync
/// test fails.
const KNOWN: &[&str] = &["title", "date", "draft", "slug", "template", "order", "redirect"];

/// Parsed frontmatter for a single page.
#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub date: Option<time::Date>,
    pub draft: bool,
    pub slug: Option<String>,
    pub template: Option<String>,
    pub order: Option<i64>,
    pub redirect: Vec<String>,
    pub taxonomies: BTreeMap<String, Vec<String>>,
    pub extra: BTreeMap<String, Value>,
}

/// The result of extracting frontmatter from a source file.
pub struct Extract {
    /// The parsed frontmatter.
    pub frontmatter: Frontmatter,
    /// The source with the `#frontmatter(...)` call spliced out, so the
    /// remainder compiles cleanly.
    pub body: String,
    /// The frontmatter argument dict literal (e.g. `(title: "x")`), reused
    /// verbatim to pass page data into a layout template.
    pub data: String,
}

impl Frontmatter {
    /// A string value from `extra` (arbitrary frontmatter), if present and a
    /// string — e.g. `description`, `summary`, `image`, `author`.
    pub fn text(&self, key: &str) -> Option<String> {
        self.extra.get(key).and_then(ValueExt::str)
    }

    /// Extract frontmatter from a source file. Returns `None` when the file has
    /// no leading `#frontmatter(...)` call. `path` names the file in errors;
    /// `config` supplies the taxonomy keys to recognize.
    pub fn extract(source: &Source, path: &Path, config: &Config) -> Result<Option<Extract>> {
        let text = source.text();
        let Some(call) = Call::find(source) else {
            return Ok(None);
        };
        let data = &text[call.args()];
        let frontmatter =
            Self::from_dict(crate::content::eval::EvalWorld::dict(data, path)?, path, config)?;
        Ok(Some(Extract {
            frontmatter,
            body: call.splice(text),
            data: data.to_owned(),
        }))
    }

    /// Interpret the evaluated frontmatter dict. A known key with a wrong-typed
    /// value is an error (never silently dropped); a configured taxonomy key
    /// collects its terms; a key that is a near-miss of a known one is a typo
    /// error; anything else passes through to `extra`.
    fn from_dict(dict: Dict, path: &Path, config: &Config) -> Result<Self> {
        let taxonomies: Vec<&str> = config.taxonomies.iter().map(|(_, t)| t.key.as_str()).collect();
        let mut fm = Self::default();
        for (key, val) in dict.iter() {
            let key = key.as_str();
            match key {
                "title" => fm.title = Some(val.string(path, key)?),
                "date" => fm.date = Some(val.date(path, key)?),
                "draft" => fm.draft = val.boolean(path, key)?,
                "slug" => fm.slug = Some(val.string(path, key)?),
                "template" => fm.template = Some(val.string(path, key)?),
                "order" => fm.order = Some(val.integer(path, key)?),
                "redirect" => fm.redirect = val.strings(path, key)?,
                _ if taxonomies.contains(&key) => {
                    fm.taxonomies.insert(key.to_owned(), val.strings(path, key)?);
                }
                _ => match Self::suggest(key, &taxonomies) {
                    Some(near) => return Err(ContentError::unknown_frontmatter(path, key, &near).into()),
                    None => {
                        fm.extra.insert(key.to_owned(), val.clone());
                    }
                },
            }
        }
        Ok(fm)
    }

    /// The known key a typo'd `key` most likely meant, if it is a near-miss of
    /// one (and not itself a real extra key). Reuses the config did-you-mean
    /// over the one known-key set (built-ins plus configured taxonomies).
    fn suggest(key: &str, taxonomies: &[&str]) -> Option<String> {
        let known: Vec<&str> = KNOWN.iter().copied().chain(taxonomies.iter().copied()).collect();
        Keys::of(&known).nearest(key).map(str::to_owned)
    }
}

/// A located `#frontmatter(...)` call within a source file.
struct Call {
    range: std::ops::Range<usize>,
    args: std::ops::Range<usize>,
}

impl Call {
    fn find(source: &Source) -> Option<Self> {
        let root = source.root();
        let markup = root.cast::<Markup>()?;
        // only a *leading* call counts: a mid-document `#frontmatter(...)` is
        // ordinary content. skip whitespace, then the first real expr must be it.
        let mut exprs = markup.exprs();
        let call = loop {
            match exprs.next()? {
                Expr::Space(_) | Expr::Parbreak(_) | Expr::Linebreak(_) => continue,
                Expr::FuncCall(c) if Self::is_frontmatter(&c) => break c,
                _ => return None,
            }
        };
        let linked = LinkedNode::new(root);
        let call_node = linked.find(call.to_untyped().span())?;
        let args_node = linked.find(call.args().to_untyped().span())?;
        let mut range = call_node.range();
        // include the leading `#` that triggered the code expression
        let text = source.text();
        if range.start > 0 && text.as_bytes().get(range.start - 1) == Some(&b'#') {
            range.start -= 1;
        }
        Some(Self {
            range,
            args: args_node.range(),
        })
    }

    fn is_frontmatter(call: &typst::syntax::ast::FuncCall) -> bool {
        matches!(call.callee(), Expr::Ident(ident) if ident.get() == "frontmatter")
    }

    fn args(&self) -> std::ops::Range<usize> {
        self.args.clone()
    }

    fn splice(&self, text: &str) -> String {
        // replace the call with as many newlines as it spanned, so body lines keep
        // their original numbers and compile diagnostics point at the real line.
        let newlines = text[self.range.clone()].bytes().filter(|&b| b == b'\n').count();
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..self.range.start]);
        out.extend(std::iter::repeat_n('\n', newlines));
        out.push_str(&text[self.range.end..]);
        out
    }
}

/// Typed accessors over an evaluated frontmatter [`Value`]. The `path`/`key`
/// parameters let a type mismatch name the file and field instead of being
/// silently dropped. [`ValueExt::str`] (infallible, for `extra` reads) is the
/// exception — a non-string there is simply "absent".
trait ValueExt {
    fn str(&self) -> Option<String>;
    fn string(&self, path: &Path, key: &str) -> Result<String>;
    fn boolean(&self, path: &Path, key: &str) -> Result<bool>;
    fn integer(&self, path: &Path, key: &str) -> Result<i64>;
    fn date(&self, path: &Path, key: &str) -> Result<time::Date>;
    fn strings(&self, path: &Path, key: &str) -> Result<Vec<String>>;
    /// This value's typst type name, for error messages.
    fn kind(&self) -> &'static str;
}

impl ValueExt for Value {
    fn str(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.to_string()),
            _ => None,
        }
    }

    fn string(&self, path: &Path, key: &str) -> Result<String> {
        self.str()
            .ok_or_else(|| ContentError::frontmatter_field(path, key, "a string", self.kind(), None).into())
    }

    fn boolean(&self, path: &Path, key: &str) -> Result<bool> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(ContentError::frontmatter_field(path, key, "a boolean", self.kind(), None).into()),
        }
    }

    fn integer(&self, path: &Path, key: &str) -> Result<i64> {
        match self {
            Value::Int(i) => Ok(*i),
            _ => Err(ContentError::frontmatter_field(path, key, "an integer", self.kind(), None).into()),
        }
    }

    fn date(&self, path: &Path, key: &str) -> Result<time::Date> {
        match self {
            Value::Datetime(Datetime::Date(d)) => Ok(*d),
            Value::Datetime(Datetime::Datetime(dt)) => Ok(dt.date()),
            _ => Err(ContentError::frontmatter_field(
                path,
                key,
                "a date",
                self.kind(),
                Some("write dates as `datetime(year: 2024, month: 1, day: 1)`"),
            )
            .into()),
        }
    }

    fn strings(&self, path: &Path, key: &str) -> Result<Vec<String>> {
        match self {
            Value::Array(arr) => Ok(arr.iter().filter_map(ValueExt::str).collect()),
            _ => Err(ContentError::frontmatter_field(
                path,
                key,
                "a list of strings",
                self.kind(),
                None,
            )
            .into()),
        }
    }

    fn kind(&self) -> &'static str {
        self.ty().long_name()
    }
}
