pub(super) mod check;
pub mod origin;

use crate::config::PermalinkCtx;
use check::{Check, Fault, ValueExt};
use origin::At;
pub use origin::Origin;
use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use typst::foundations::{Datetime, Dict, Module, Value};
use typst::syntax::{
    Source,
    ast::{Expr, Markup},
};

use crate::codegen;
use crate::config::dispatch::Keys;
use crate::config::{Config, FieldType};
use crate::error::{ContentError, Result, SchemaError};

/// A frontmatter field parser: reads the evaluated value into its slot on `fm`,
/// naming and underlining the key on a type mismatch (never silently dropped).
type Field = fn(fm: &mut Frontmatter, value: &Value, at: At<'_>) -> Result<()>;

/// What a built-in key holds, as a constructor rather than a value: a
/// [`FieldType`] owns the types it wraps, which no constant can build.
type Shape = fn() -> FieldType;

/// The recognized built-in frontmatter keys, what each holds, and how each
/// parses: the single source of truth for dispatch, the typo suggester, and the
/// check that a collection's schema cannot declare a built-in as something it
/// can never be. A new key is one row here and the three can't drift (taxonomy
/// keys are configured, so they are recognized dynamically, not listed).
/// Mirrors `config::dispatch`'s tables.
const FIELDS: &[(&str, Shape, Field)] = &[
    (
        "title",
        || FieldType::Str,
        |fm, v, at| {
            fm.title = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "date",
        || FieldType::Date,
        |fm, v, at| {
            fm.date = Some(v.date(at)?);
            Ok(())
        },
    ),
    (
        "updated",
        || FieldType::Date,
        |fm, v, at| {
            fm.updated = Some(v.date(at)?);
            Ok(())
        },
    ),
    (
        "expiry",
        || FieldType::Date,
        |fm, v, at| {
            fm.expiry = Some(v.date(at)?);
            Ok(())
        },
    ),
    (
        "draft",
        || FieldType::Bool,
        |fm, v, at| {
            fm.draft = v.boolean(at)?;
            Ok(())
        },
    ),
    (
        "slug",
        || FieldType::Str,
        |fm, v, at| {
            fm.slug = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "path",
        || FieldType::Str,
        |fm, v, at| {
            fm.path = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "lang",
        || FieldType::Str,
        |fm, v, at| {
            fm.lang = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "translation",
        || FieldType::Str,
        |fm, v, at| {
            fm.translation = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "template",
        || FieldType::Str,
        |fm, v, at| {
            fm.template = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "order",
        || FieldType::Int,
        |fm, v, at| {
            fm.order = Some(v.integer(at)?);
            Ok(())
        },
    ),
    (
        "redirect",
        || FieldType::List(Box::new(FieldType::Str)),
        |fm, v, at| {
            fm.redirect = v.strings(at)?;
            Ok(())
        },
    ),
    (
        "source",
        || FieldType::Str,
        |fm, v, at| {
            fm.source = Some(v.string(at)?);
            Ok(())
        },
    ),
];

/// Parsed frontmatter for a single page.
///
/// Serializable so discovery can cache it and skip re-evaluating the page's
/// typst module on an unchanged build. `extra` holds arbitrary frontmatter as
/// [`codegen::Value`] rather than a raw typst `Value`: it renders to the same
/// generated source, keeps string content readable for [`Frontmatter::text`],
/// and, unlike a typst runtime value, round-trips through the cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub date: Option<time::Date>,
    /// When the page last changed materially, if it has since being published.
    ///
    /// Distinct from `date`, which is when it was *published* and which orders
    /// every listing: a 2023 guide rewritten today is still a 2023 post, but a
    /// crawler and a feed reader both need to hear that it moved.
    pub updated: Option<time::Date>,
    /// The date this page stops building: an event that has happened, a call
    /// for papers that has closed, an offer that has ended.
    ///
    /// The other end of the window `content { future }` opens. A page is
    /// excluded from the day *after* this date, so `expiry` names the last day
    /// it is published rather than the first day it is not.
    pub expiry: Option<time::Date>,
    pub draft: bool,
    pub slug: Option<String>,
    /// The exact URL this page publishes at, replacing its collection's
    /// permalink pattern and its slug both.
    ///
    /// The escape hatch a migration needs: a site arriving from elsewhere can
    /// copy each old URL onto the page that answers it and match the old URL set
    /// by construction, instead of reverse-engineering one pattern per shape and
    /// declaring `redirect` for whatever the pattern missed. Hugo (`url`), Zola
    /// (`path`), Jekyll and Eleventy (`permalink`) all have it.
    ///
    /// It does not touch the page's identity: `slug` still pairs translations,
    /// so an edition may take its own path without becoming a separate page.
    pub path: Option<String>,
    /// Explicit language override; beats the filename suffix and the default
    /// `lang`. Only meaningful on a multi-language site.
    pub lang: Option<String>,
    /// An explicit key pairing this page with its editions in other languages.
    ///
    /// Editions pair on `collection/slug` by default, which is why a French
    /// edition had to keep the English slug: give it a French one and it became
    /// a standalone page instead, losing the switcher and its `hreflang`
    /// alternates. Naming the same key on both restores the pairing and frees
    /// the slug.
    pub translation: Option<String>,
    pub template: Option<String>,
    pub order: Option<i64>,
    pub redirect: Vec<String>,
    /// The name of a `paths { sources { } }` entry whose file is this page's
    /// body, in place of the one written under the frontmatter.
    ///
    /// A *name*, never a path: the config declares which files may be read and
    /// under what name, and a page can only ask for one of those. That is what
    /// keeps a key that reads a file off disk from being a way for content to
    /// name any file on the machine.
    pub source: Option<String>,
    pub taxonomies: BTreeMap<String, Vec<String>>,
    pub extra: BTreeMap<String, codegen::Value>,
}

impl Frontmatter {
    /// The permalink context for a page with this frontmatter, at an
    /// already-resolved `slug` (frontmatter-else-stem precedence is decided
    /// once, in `Page::load`).
    pub fn permalink(&self, collection: &str, slug: &str, path: Vec<String>) -> PermalinkCtx {
        PermalinkCtx {
            slug: slug.to_owned(),
            collection: collection.to_owned(),
            path,
            date: self.date,
            order: self.order,
        }
    }

    /// When this page last changed: its `updated`, else its publish `date`.
    ///
    /// The single answer to "how recent is this", so the sitemap's `lastmod` and
    /// a feed entry's `updated` cannot disagree. Both used to read `date` alone,
    /// which meant a rewritten page told crawlers it had not changed since it
    /// was first published, and never got recrawled or resurfaced.
    pub fn modified(&self) -> Option<time::Date> {
        self.updated.or(self.date)
    }

    /// The page's one-line summary, from `description` or its `summary` alias.
    ///
    /// One rule, because three consumers ask the same question and a reader who
    /// sees a preview in a `<meta>` tag but not in the feed would be reading a
    /// bug: the head tags, the feed entry, and the announced record.
    pub fn description(&self) -> Option<String> {
        self.text("description").or_else(|| self.text("summary"))
    }

    /// A string value from `extra` (arbitrary frontmatter), if present and a
    /// string, e.g. `description`, `summary`, `image`, `author`.
    pub fn text(&self, key: &str) -> Option<String> {
        self.extra
            .get(key)
            .and_then(codegen::Value::as_str)
            .map(str::to_owned)
    }

    /// Reject the removed `#frontmatter(..)` call form with a migration error.
    /// A syntax-tree check, run *before* evaluation: the call no longer
    /// evaluates (`frontmatter` is undefined), and "unknown variable" would
    /// say nothing about the new syntax.
    pub fn check(source: &Source, path: &Path) -> Result<()> {
        match Self::legacy_call(source) {
            true => Err(ContentError::frontmatter_call(path).into()),
            false => Ok(()),
        }
    }

    /// Whether the source opens with the pre-export `#frontmatter(..)` call
    /// form, recognized in the syntax tree purely to point migration at the
    /// binding syntax (the call itself no longer evaluates: `frontmatter` is
    /// undefined).
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

    /// What a built-in frontmatter key holds, if `key` is one. Read by the
    /// config parser, so a collection schema declaring a built-in as something
    /// it can never be fails at the line that wrote it rather than on every
    /// page of the collection.
    pub fn builtin(key: &str) -> Option<FieldType> {
        FIELDS
            .iter()
            .find(|(name, ..)| *name == key)
            .map(|&(_, shape, _)| shape())
    }

    /// Read a page's frontmatter from its evaluated module's `frontmatter`
    /// export (`#let frontmatter = (..)`). Returns `None` when the module
    /// exports none. `origin` names the file in errors and supplies the
    /// collection whose schema applies; `config` supplies the taxonomy keys to
    /// recognize.
    pub fn extract(module: &Module, origin: &Origin, config: &Config) -> Result<Option<Self>> {
        let Some(binding) = module.scope().get("frontmatter") else {
            // A collection requiring fields is not satisfied by declaring no
            // frontmatter at all: the emptiest page is exactly the one the
            // schema exists to catch.
            Self::validate(&Dict::new(), origin, config)?;
            return Ok(None);
        };
        let value = binding.read();
        let Value::Dict(dict) = value else {
            return Err(ContentError::frontmatter_not_dict(origin.path, value).into());
        };
        Self::from_dict(dict, origin, config).map(Some)
    }

    /// Interpret the evaluated frontmatter dict. A known key with a wrong-typed
    /// value is an error (never silently dropped); a configured taxonomy key
    /// collects its terms; a key that is a near-miss of a known one is a typo
    /// error; anything else passes through to `extra`.
    pub(crate) fn from_dict(dict: &Dict, origin: &Origin, config: &Config) -> Result<Self> {
        // Before reading, not after: a schema violation on a built-in key would
        // otherwise surface as whatever the built-in reader made of the value,
        // which says nothing about the collection that asked for it.
        Self::validate(dict, origin, config)?;
        let taxonomies: Vec<&str> = config
            .content
            .taxonomies
            .iter()
            .map(|(_, t)| t.key.as_str())
            .collect();
        let mut fm = Self::default();
        for (key, val) in dict {
            let key = key.as_str();
            let at = At::new(origin, key);
            match FIELDS.iter().find(|(name, ..)| *name == key) {
                Some((.., parse)) => parse(&mut fm, val, at)?,
                None if taxonomies.contains(&key) => {
                    fm.taxonomies.insert(key.to_owned(), val.strings(at)?);
                }
                None => match Self::suggest(key, &taxonomies) {
                    Some(near) => return Err(at.unknown(&near)),
                    None => {
                        fm.extra.insert(key.to_owned(), codegen::Value::from(val));
                    }
                },
            }
        }
        Ok(fm)
    }

    /// Hold the declared dict to the schema of the collection the page belongs
    /// to. A collection declaring none constrains nothing, which is the default
    /// and the whole of the previous behaviour.
    fn validate(dict: &Dict, origin: &Origin, config: &Config) -> Result<()> {
        let Some(fault) = Check::default().dict(config.schema(origin.collection), dict) else {
            return Ok(());
        };
        let (source, key) = (origin.text(), fault.key());
        let error = match &fault {
            // What is missing has no place of its own, so the span points at
            // the innermost value that does exist: the dictionary that should
            // have held it, or the binding itself for a top-level field.
            Fault::Missing { want, .. } => SchemaError::missing(
                origin.path,
                source,
                origin.span(fault.parent()),
                origin.collection,
                &key,
                want,
            ),
            Fault::Mismatch { want, got, .. } => SchemaError::mismatch(
                origin.path,
                source,
                origin.span(fault.path()),
                origin.collection,
                &key,
                want,
                got,
            ),
        };
        Err(error.into())
    }

    /// The known key a typo'd `key` most likely meant, if it is a near-miss of
    /// one (and not itself a real extra key). Reuses the config did-you-mean
    /// over the one known-key set (built-ins plus configured taxonomies).
    fn suggest(key: &str, taxonomies: &[&str]) -> Option<String> {
        let known: Vec<&str> = FIELDS
            .iter()
            .map(|(name, ..)| *name)
            .chain(taxonomies.iter().copied())
            .collect();
        Keys::of(&known).nearest(key).map(str::to_owned)
    }
}

impl ValueExt for Value {
    fn str(&self) -> Option<String> {
        match self {
            Self::Str(s) => Some(s.to_string()),
            _ => None,
        }
    }

    fn string(&self, at: At<'_>) -> Result<String> {
        self.str()
            .ok_or_else(|| at.field(FieldType::Str.words().article, self.kind(), None))
    }

    fn boolean(&self, at: At<'_>) -> Result<bool> {
        match self {
            Self::Bool(b) => Ok(*b),
            _ => Err(at.field(FieldType::Bool.words().article, self.kind(), None)),
        }
    }

    fn integer(&self, at: At<'_>) -> Result<i64> {
        match self {
            Self::Int(i) => Ok(*i),
            _ => Err(at.field(FieldType::Int.words().article, self.kind(), None)),
        }
    }

    fn date(&self, at: At<'_>) -> Result<time::Date> {
        match self {
            Self::Datetime(Datetime::Date(d)) => Ok(*d),
            Self::Datetime(Datetime::Datetime(dt)) => Ok(dt.date()),
            // KDL has no date literal, so a markdown page writes the ISO day as
            // a string. A typst page may too, rather than spelling out the call.
            Self::Str(text) => text
                .as_str()
                .parse::<crate::content::date::Iso>()
                .map(|iso| iso.0)
                .map_err(|()| {
                    at.field(
                        FieldType::Date.words().article,
                        "a string that is not an ISO day",
                        Some("write it as `\"2024-01-01\"`, or as `datetime(year: 2024, month: 1, day: 1)`"),
                    )
                }),
            _ => Err(at.field(
                FieldType::Date.words().article,
                self.kind(),
                Some("write dates as `\"2024-01-01\"` or `datetime(year: 2024, month: 1, day: 1)`"),
            )),
        }
    }

    fn strings(&self, at: At<'_>) -> Result<Vec<String>> {
        // a wrong-typed *element* is an error too, never silently dropped,
        // same as every scalar accessor here, and underlined where it sits
        // rather than under the whole list.
        let wrong = |at: At<'_>, kind| at.field(FieldType::Str.words().list, kind, None);
        match self {
            Self::Array(arr) => arr
                .iter()
                .enumerate()
                .map(|(i, v)| v.str().ok_or_else(|| wrong(at.nth(i), v.kind())))
                .collect(),
            _ => Err(wrong(at, self.kind())),
        }
    }

    fn kind(&self) -> &'static str {
        self.ty().long_name()
    }
}
