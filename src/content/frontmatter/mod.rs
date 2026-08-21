pub(crate) mod check;
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

/// One file a build generates *about* a page, which the page may decline.
#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Generated {
    /// The page's entry in `sitemap.xml`.
    Sitemap,
    /// Its entry in every syndication feed.
    Feed,
    /// Its entry in the client-side search index.
    Search,
    /// Its social card, and the `og:image` that names one.
    Card,
    /// Its PDF, and the `<link rel="alternate">` that points at one.
    Pdf,
}

impl crate::config::Named for Generated {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("sitemap", Self::Sitemap),
        ("feed", Self::Feed),
        ("search", Self::Search),
        ("card", Self::Card),
        ("pdf", Self::Pdf),
    ];
}

/// A frontmatter field parser: reads the evaluated value into its slot on `fm`,
/// naming and underlining the key on a type mismatch (never silently dropped).
type Field = fn(fm: &mut Frontmatter, value: &Value, at: At<'_>) -> Result<()>;

/// What a built-in key holds, as a constructor rather than a value: a
/// [`FieldType`] owns the types it wraps, which no constant can build.
type Shape = fn() -> FieldType;

/// The recognized built-in frontmatter keys, what each holds, and how each
/// parses: the one table dispatch, the typo suggester and the schema check all
/// read. Taxonomy keys are configured, so they are recognized dynamically.
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
            fm.path = Some(v.url(at)?);
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
            fm.redirect = v.urls(at)?;
            Ok(())
        },
    ),
    (
        Frontmatter::SOURCE,
        || FieldType::Str,
        |fm, v, at| {
            fm.source = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "exclude",
        || FieldType::List(Box::new(FieldType::Str)),
        |fm, v, at| {
            use crate::config::Named;
            let valid = Generated::names();
            for (index, name) in v.strings(at)?.into_iter().enumerate() {
                let generated =
                    Generated::of(&name).ok_or_else(|| at.nth(index).name(&name, &valid))?;
                if !fm.exclude.contains(&generated) {
                    fm.exclude.push(generated);
                }
            }
            Ok(())
        },
    ),
    (
        "description",
        || FieldType::Str,
        |fm, v, at| {
            fm.description = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "summary",
        || FieldType::Str,
        |fm, v, at| {
            fm.summary = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "image",
        || FieldType::Str,
        |fm, v, at| {
            fm.image = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "alt",
        || FieldType::Str,
        |fm, v, at| {
            fm.alt = Some(v.string(at)?);
            Ok(())
        },
    ),
    (
        "author",
        || FieldType::Str,
        |fm, v, at| {
            fm.author = Some(v.string(at)?);
            Ok(())
        },
    ),
];

/// Parsed frontmatter for a single page. `extra` holds arbitrary keys as
/// [`codegen::Value`] rather than a typst `Value` because only that round-trips
/// through the discovery cache.
#[derive(Hash, Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub date: Option<time::Date>,
    /// When the page last changed materially, distinct from `date`, which is
    /// when it was published and which orders every listing.
    pub updated: Option<time::Date>,
    /// The last day this page is published; it stops building the day after.
    pub expiry: Option<time::Date>,
    pub draft: bool,
    pub slug: Option<String>,
    /// The exact URL this page publishes at, replacing its collection's
    /// permalink pattern and its slug both. It does not touch the page's
    /// identity: `slug` still pairs translations.
    pub path: Option<String>,
    /// Explicit language override; beats the filename suffix and the default
    /// `lang`. Only meaningful on a multi-language site.
    pub lang: Option<String>,
    /// An explicit key pairing this page with its editions in other languages,
    /// which otherwise pair on `collection/slug`.
    pub translation: Option<String>,
    pub template: Option<String>,
    pub order: Option<i64>,
    pub redirect: Vec<String>,
    /// The name of a `paths { sources { } }` entry whose file is this page's
    /// body, in place of the one written under the frontmatter. A *name*, never
    /// a path, so a page can only ask for a file the config already declared.
    pub source: Option<String>,
    /// The page's one-line summary, read by the head tags, the feed entry, the
    /// listing preview and the announced record.
    pub description: Option<String>,
    /// The alias [`Frontmatter::description`] falls back to, for the sites that
    /// spell it this way.
    pub summary: Option<String>,
    /// The page's own social image, which always wins over a generated card.
    pub image: Option<String>,
    /// What that image shows. Empty marks it decorative, as it does in markup.
    pub alt: Option<String>,
    /// Who wrote this page, over the site's default for its language.
    pub author: Option<String>,
    /// The generated files this page declines to appear in.
    pub exclude: Vec<Generated>,
    pub taxonomies: BTreeMap<String, Vec<String>>,
    pub extra: BTreeMap<String, codegen::Value>,
}

impl Frontmatter {
    /// The name a page binds its frontmatter under, and every reader looks up.
    pub(crate) const EXPORT: &'static str = "frontmatter";

    /// The local alias a generated wrapper imports
    /// [`EXPORT`](Frontmatter::EXPORT) under, `__`-prefixed so nothing a page
    /// or a template binds can shadow it.
    pub(crate) const ALIAS: &'static str = "__data";

    /// The key naming a declared source.
    pub(crate) const SOURCE: &'static str = "source";

    /// The permalink context for a page with this frontmatter, at an
    /// already-resolved `slug`.
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
    pub fn modified(&self) -> Option<time::Date> {
        self.updated.or(self.date)
    }

    /// Whether this page declined the file a build would generate about it.
    pub fn excludes(&self, what: Generated) -> bool {
        self.exclude.contains(&what)
    }

    /// The page's one-line summary: `description`, else its `summary` alias.
    pub fn blurb(&self) -> Option<&str> {
        self.description.as_deref().or(self.summary.as_deref())
    }

    /// A string value from `extra`, the frontmatter this crate does not name,
    /// if present and a string.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.extra.get(key).and_then(codegen::Value::as_str)
    }

    /// Reject the removed `#frontmatter(..)` call form with a migration error,
    /// before evaluation raises an "unknown variable" that says nothing about
    /// the new syntax.
    pub fn check(source: &Source, path: &Path) -> Result<()> {
        if Self::legacy_call(source) {
            Err(ContentError::frontmatter_call(path).into())
        } else {
            Ok(())
        }
    }

    /// Whether the source opens with the pre-export `#frontmatter(..)` call
    /// form.
    fn legacy_call(source: &Source) -> bool {
        let Some(markup) = source.root().cast::<Markup>() else {
            return false;
        };
        markup
            .exprs()
            .find(|e| !matches!(e, Expr::Space(_) | Expr::Parbreak(_) | Expr::Linebreak(_)))
            .is_some_and(|first| match first {
                Expr::FuncCall(call) => {
                    matches!(call.callee(), Expr::Ident(ident) if ident.get() == Self::EXPORT)
                }
                _ => false,
            })
    }

    /// What a built-in frontmatter key holds, if `key` is one. Read by the
    /// config parser, so a schema declaring a built-in as something it can
    /// never be fails at the line that wrote it.
    pub fn builtin(key: &str) -> Option<FieldType> {
        FIELDS
            .iter()
            .find(|(name, ..)| *name == key)
            .map(|&(_, shape, _)| shape())
    }

    /// Read a page's frontmatter from its evaluated module's `frontmatter`
    /// export, with `false` where the module exports none.
    ///
    /// A module exporting none is read from the empty dict rather than skipped:
    /// it is held to its collection's schema either way, and the defaults that
    /// schema declares are what the wrapper lays under the page regardless.
    pub fn extract(module: &Module, origin: &Origin, config: &Config) -> Result<(Self, bool)> {
        let Some(binding) = module.scope().get(Self::EXPORT) else {
            return Ok((Self::from_dict(&Dict::new(), origin, config)?, false));
        };
        let value = binding.read();
        let Value::Dict(dict) = value else {
            return Err(ContentError::frontmatter_not_dict(origin.path, value).into());
        };
        Ok((Self::from_dict(dict, origin, config)?, true))
    }

    /// Interpret the evaluated frontmatter dict. A known key with a wrong-typed
    /// value is an error (never silently dropped); a configured taxonomy key
    /// collects its terms; a key that is a near-miss of a known one is a typo
    /// error; anything else passes through to `extra`.
    pub(crate) fn from_dict(dict: &Dict, origin: &Origin, config: &Config) -> Result<Self> {
        Self::validate(dict, origin, config)?;
        let taxonomies: Vec<&str> = config
            .content
            .taxonomies
            .iter()
            .map(|(_, t)| t.key.as_str())
            .collect();
        let declared: Vec<&str> = config
            .schema(origin.collection)
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        let mut fm = Self::default();
        for (key, val) in dict {
            fm.entry(key.as_str(), val, origin, &taxonomies, &declared)?;
        }
        fm.defaults(dict, origin, config, &taxonomies, &declared)?;
        Ok(fm)
    }

    /// One frontmatter entry into its slot: a built-in key through its own
    /// parser, a configured taxonomy into its terms, anything else into
    /// `extra`, and a near-miss of a known key into an error.
    fn entry(
        &mut self,
        key: &str,
        val: &Value,
        origin: &Origin,
        taxonomies: &[&str],
        declared: &[&str],
    ) -> Result<()> {
        let at = At::new(origin, key);
        match FIELDS.iter().find(|(name, ..)| *name == key) {
            Some((.., parse)) => parse(self, val, at)?,
            None if taxonomies.contains(&key) => {
                self.taxonomies.insert(key.to_owned(), val.strings(at)?);
            }
            None if declared.contains(&key) => {
                self.extra.insert(key.to_owned(), codegen::Value::from(val));
            }
            None => match Self::suggest(key, taxonomies) {
                Some(near) => return Err(at.unknown(&near)),
                None => {
                    self.extra.insert(key.to_owned(), codegen::Value::from(val));
                }
            },
        }
        Ok(())
    }

    /// Fill in every declared field the page left out and the schema gives a
    /// default, through the same route a written value takes: a default is what
    /// the page would have written, so it lands where that would have.
    fn defaults(
        &mut self,
        dict: &Dict,
        origin: &Origin,
        config: &Config,
        taxonomies: &[&str],
        declared: &[&str],
    ) -> Result<()> {
        for (key, field) in config.schema(origin.collection) {
            let Some(default) = &field.default else {
                continue;
            };
            if dict.get(key.as_str()).is_ok() {
                continue;
            }
            let value = Value::from(default);
            self.entry(key, &value, origin, taxonomies, declared)?;
        }
        Ok(())
    }

    /// Hold the declared dict to the schema of the collection the page belongs
    /// to. A collection declaring none constrains nothing.
    fn validate(dict: &Dict, origin: &Origin, config: &Config) -> Result<()> {
        let Some(fault) = Check::default().dict(config.schema(origin.collection), dict) else {
            return Ok(());
        };
        let (source, key) = (origin.text(), fault.key());
        let span = origin.span(fault.steps());
        let error = match &fault {
            Fault::Missing { want, .. } => {
                SchemaError::missing(origin.path, source, span, origin.collection, &key, want)
            }
            Fault::Mismatch { want, got, .. } => SchemaError::mismatch(
                origin.path,
                source,
                span,
                origin.collection,
                &key,
                want,
                got,
            ),
            Fault::Refused { want, .. } => {
                SchemaError::refused(origin.path, source, span, origin.collection, &key, want)
            }
        };
        Err(error.into())
    }

    /// The known key a typo'd `key` most likely meant, if it is a near-miss of
    /// one. The suggestion rejects the key rather than annotating an error, so
    /// a false positive costs a site a legal key.
    fn suggest(key: &str, taxonomies: &[&str]) -> Option<String> {
        let known: Vec<&str> = FIELDS
            .iter()
            .map(|(name, ..)| *name)
            .chain(taxonomies.iter().copied())
            .collect();
        Keys::of(&known)
            .nearest(key)
            .filter(|near| !Self::extends(key, near))
            .map(str::to_owned)
    }

    /// Whether `key` is a known key with more written after it, which makes it
    /// a different word rather than a slip: `authors` beside `author`. One
    /// direction only, so `tag` beside `tags` stays a typo.
    fn extends(key: &str, known: &str) -> bool {
        key.len() > known.len() && key.starts_with(known)
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
            .ok_or_else(|| at.field(FieldType::Str.words().article, &self.kind(), None))
    }

    fn boolean(&self, at: At<'_>) -> Result<bool> {
        match self {
            Self::Bool(b) => Ok(*b),
            _ => Err(at.field(FieldType::Bool.words().article, &self.kind(), None)),
        }
    }

    fn integer(&self, at: At<'_>) -> Result<i64> {
        match self {
            Self::Int(i) => Ok(*i),
            _ => Err(at.field(FieldType::Int.words().article, &self.kind(), None)),
        }
    }

    fn date(&self, at: At<'_>) -> Result<time::Date> {
        match self {
            Self::Datetime(Datetime::Date(d)) => Ok(*d),
            Self::Datetime(Datetime::Datetime(dt)) => Ok(dt.date()),
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
                &self.kind(),
                Some("write dates as `\"2024-01-01\"` or `datetime(year: 2024, month: 1, day: 1)`"),
            )),
        }
    }

    fn strings(&self, at: At<'_>) -> Result<Vec<String>> {
        let wrong = |at: At<'_>, kind: &str| at.field(FieldType::Str.words().list, kind, None);
        match self {
            Self::Array(arr) => arr
                .iter()
                .enumerate()
                .map(|(i, v)| v.str().ok_or_else(|| wrong(at.nth(i), &v.kind())))
                .collect(),
            _ => Err(wrong(at, &self.kind())),
        }
    }

    fn url(&self, at: At<'_>) -> Result<String> {
        let url = self.string(at)?;
        if Config::traverses(&url) {
            return Err(at.traversal());
        }
        Ok(url)
    }

    fn urls(&self, at: At<'_>) -> Result<Vec<String>> {
        let urls = self.strings(at)?;
        urls.iter()
            .position(|url| Config::traverses(url))
            .map_or_else(|| Ok(urls), |i| Err(at.nth(i).traversal()))
    }

    /// What this value is, with the article that reads before it: `a string`,
    /// `an integer`. The article travels with the noun because the message
    /// reads `but is {got}`.
    fn kind(&self) -> String {
        let name = self.ty().long_name();
        let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {name}")
    }
}
