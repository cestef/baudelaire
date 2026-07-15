use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use typst::foundations::{Datetime, Dict, Module, Value};
use typst::syntax::{
    Source,
    ast::{Expr, Markup},
};

use crate::codegen;
use crate::config::Config;
use crate::config::dispatch::Keys;
use crate::error::{ContentError, Result};

/// A frontmatter field parser: reads the evaluated value into its slot on `fm`,
/// naming `path`/`key` on a type mismatch (never silently dropped).
type Field = fn(fm: &mut Frontmatter, value: &Value, path: &Path, key: &str) -> Result<()>;

/// The recognized built-in frontmatter keys and how each parses — the single
/// source of truth for both dispatch and the typo suggester, so a new key is one
/// row here and the two can't drift (taxonomy keys are configured, so they are
/// recognized dynamically, not listed). Mirrors `config::dispatch`'s tables.
const FIELDS: &[(&str, Field)] = &[
    ("title", |fm, v, p, k| {
        fm.title = Some(v.string(p, k)?);
        Ok(())
    }),
    ("date", |fm, v, p, k| {
        fm.date = Some(v.date(p, k)?);
        Ok(())
    }),
    ("draft", |fm, v, p, k| {
        fm.draft = v.boolean(p, k)?;
        Ok(())
    }),
    ("slug", |fm, v, p, k| {
        fm.slug = Some(v.string(p, k)?);
        Ok(())
    }),
    ("template", |fm, v, p, k| {
        fm.template = Some(v.string(p, k)?);
        Ok(())
    }),
    ("order", |fm, v, p, k| {
        fm.order = Some(v.integer(p, k)?);
        Ok(())
    }),
    ("redirect", |fm, v, p, k| {
        fm.redirect = v.strings(p, k)?;
        Ok(())
    }),
];

/// Parsed frontmatter for a single page.
///
/// Serializable so discovery can cache it and skip re-evaluating the page's
/// typst module on an unchanged build. `extra` holds arbitrary frontmatter as
/// [`codegen::Value`] rather than a raw typst `Value`: it renders to the same
/// generated source, keeps string content readable for [`Frontmatter::text`],
/// and — unlike a typst runtime value — round-trips through the cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub date: Option<time::Date>,
    pub draft: bool,
    pub slug: Option<String>,
    pub template: Option<String>,
    pub order: Option<i64>,
    pub redirect: Vec<String>,
    pub taxonomies: BTreeMap<String, Vec<String>>,
    pub extra: BTreeMap<String, codegen::Value>,
}

impl Frontmatter {
    /// A string value from `extra` (arbitrary frontmatter), if present and a
    /// string — e.g. `description`, `summary`, `image`, `author`.
    pub fn text(&self, key: &str) -> Option<String> {
        self.extra
            .get(key)
            .and_then(codegen::Value::as_str)
            .map(str::to_owned)
    }

    /// Reject the removed `#frontmatter(..)` call form with a migration error.
    /// A syntax-tree check, run *before* evaluation — the call no longer
    /// evaluates (`frontmatter` is undefined), and "unknown variable" would
    /// say nothing about the new syntax.
    pub fn check(source: &Source, path: &Path) -> Result<()> {
        match legacy_call(source) {
            true => Err(ContentError::frontmatter_call(path).into()),
            false => Ok(()),
        }
    }

    /// Read a page's frontmatter from its evaluated module's `frontmatter`
    /// export (`#let frontmatter = (..)`). Returns `None` when the module
    /// exports none. `path` names the file in errors; `config` supplies the
    /// taxonomy keys to recognize.
    pub fn extract(module: &Module, path: &Path, config: &Config) -> Result<Option<Self>> {
        let Some(binding) = module.scope().get("frontmatter") else {
            return Ok(None);
        };
        let value = binding.read();
        let Value::Dict(dict) = value else {
            return Err(ContentError::frontmatter_not_dict(path, value).into());
        };
        Self::from_dict(dict.clone(), path, config).map(Some)
    }

    /// Interpret the evaluated frontmatter dict. A known key with a wrong-typed
    /// value is an error (never silently dropped); a configured taxonomy key
    /// collects its terms; a key that is a near-miss of a known one is a typo
    /// error; anything else passes through to `extra`.
    fn from_dict(dict: Dict, path: &Path, config: &Config) -> Result<Self> {
        let taxonomies: Vec<&str> = config
            .taxonomies
            .iter()
            .map(|(_, t)| t.key.as_str())
            .collect();
        let mut fm = Self::default();
        for (key, val) in dict.iter() {
            let key = key.as_str();
            match FIELDS.iter().find(|(name, _)| *name == key) {
                Some((_, parse)) => parse(&mut fm, val, path, key)?,
                None if taxonomies.contains(&key) => {
                    fm.taxonomies
                        .insert(key.to_owned(), val.strings(path, key)?);
                }
                None => match Self::suggest(key, &taxonomies) {
                    Some(near) => {
                        return Err(ContentError::unknown_frontmatter(path, key, &near).into());
                    }
                    None => {
                        fm.extra.insert(key.to_owned(), codegen::Value::from(val));
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
        let known: Vec<&str> = FIELDS
            .iter()
            .map(|(name, _)| *name)
            .chain(taxonomies.iter().copied())
            .collect();
        Keys::of(&known).nearest(key).map(str::to_owned)
    }
}

/// Whether the source opens with the pre-export `#frontmatter(..)` call form —
/// recognized in the syntax tree purely to point migration at the binding
/// syntax (the call itself no longer evaluates: `frontmatter` is undefined).
fn legacy_call(source: &Source) -> bool {
    let Some(markup) = source.root().cast::<Markup>() else {
        return false;
    };
    markup
        .exprs()
        .find(|e| !matches!(e, Expr::Space(_) | Expr::Parbreak(_) | Expr::Linebreak(_)))
        .is_some_and(|first| match first {
            Expr::FuncCall(call) => {
                matches!(call.callee(), Expr::Ident(ident) if ident.get() == "frontmatter")
            }
            _ => false,
        })
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
        self.str().ok_or_else(|| {
            ContentError::frontmatter_field(path, key, "a string", self.kind(), None).into()
        })
    }

    fn boolean(&self, path: &Path, key: &str) -> Result<bool> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(
                ContentError::frontmatter_field(path, key, "a boolean", self.kind(), None).into(),
            ),
        }
    }

    fn integer(&self, path: &Path, key: &str) -> Result<i64> {
        match self {
            Value::Int(i) => Ok(*i),
            _ => Err(
                ContentError::frontmatter_field(path, key, "an integer", self.kind(), None).into(),
            ),
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
        // a wrong-typed *element* is an error too — never silently dropped,
        // same as every scalar accessor here.
        let wrong = |kind| {
            ContentError::frontmatter_field(path, key, "a list of strings", kind, None).into()
        };
        match self {
            Value::Array(arr) => arr
                .iter()
                .map(|v| v.str().ok_or_else(|| wrong(v.kind())))
                .collect(),
            _ => Err(wrong(self.kind())),
        }
    }

    fn kind(&self) -> &'static str {
        self.ty().long_name()
    }
}
