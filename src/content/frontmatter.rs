use crate::config::PermalinkCtx;
use std::collections::BTreeMap;
use std::path::Path;

use miette::SourceSpan;
use serde::{Deserialize, Serialize};
use typst::foundations::{Datetime, Dict, Module, Value};
use typst::syntax::{
    Source, SyntaxNode,
    ast::{AstNode, DictItem, Expr, LetBinding, Markup},
};

use crate::codegen;
use crate::config::dispatch::Keys;
use crate::config::{Config, FieldType};
use crate::error::{ContentError, Result, SchemaError};

/// A frontmatter field parser: reads the evaluated value into its slot on `fm`,
/// naming `path`/`key` on a type mismatch (never silently dropped).
type Field = fn(fm: &mut Frontmatter, value: &Value, path: &Path, key: &str) -> Result<()>;

/// The recognized built-in frontmatter keys, what each holds, and how each
/// parses: the single source of truth for dispatch, the typo suggester, and the
/// check that a collection's schema cannot declare a built-in as something it
/// can never be. A new key is one row here and the three can't drift (taxonomy
/// keys are configured, so they are recognized dynamically, not listed).
/// Mirrors `config::dispatch`'s tables.
const FIELDS: &[(&str, FieldType, Field)] = &[
    ("title", FieldType::Str, |fm, v, p, k| {
        fm.title = Some(v.string(p, k)?);
        Ok(())
    }),
    ("date", FieldType::Date, |fm, v, p, k| {
        fm.date = Some(v.date(p, k)?);
        Ok(())
    }),
    ("updated", FieldType::Date, |fm, v, p, k| {
        fm.updated = Some(v.date(p, k)?);
        Ok(())
    }),
    ("expiry", FieldType::Date, |fm, v, p, k| {
        fm.expiry = Some(v.date(p, k)?);
        Ok(())
    }),
    ("draft", FieldType::Bool, |fm, v, p, k| {
        fm.draft = v.boolean(p, k)?;
        Ok(())
    }),
    ("slug", FieldType::Str, |fm, v, p, k| {
        fm.slug = Some(v.string(p, k)?);
        Ok(())
    }),
    ("lang", FieldType::Str, |fm, v, p, k| {
        fm.lang = Some(v.string(p, k)?);
        Ok(())
    }),
    ("translation", FieldType::Str, |fm, v, p, k| {
        fm.translation = Some(v.string(p, k)?);
        Ok(())
    }),
    ("template", FieldType::Str, |fm, v, p, k| {
        fm.template = Some(v.string(p, k)?);
        Ok(())
    }),
    ("order", FieldType::Int, |fm, v, p, k| {
        fm.order = Some(v.integer(p, k)?);
        Ok(())
    }),
    ("redirect", FieldType::List, |fm, v, p, k| {
        fm.redirect = v.strings(p, k)?;
        Ok(())
    }),
];

/// Where a frontmatter dict came from: the page source its spans point into,
/// the path errors name it by, and the collection whose schema constrains it.
///
/// One value rather than three parameters threaded through extraction, because
/// every one of them exists to make a diagnostic precise and they are always
/// needed together.
pub struct Origin<'a> {
    source: &'a Source,
    path: &'a Path,
    collection: &'a str,
}

impl<'a> Origin<'a> {
    pub fn new(source: &'a Source, path: &'a Path, collection: &'a str) -> Self {
        Self {
            source,
            path,
            collection,
        }
    }

    /// The byte span of `key`'s value inside `#let frontmatter = (..)`, or of
    /// the binding itself when `key` is `None`: a field that is absent has
    /// nowhere of its own to point at.
    ///
    /// `None` when the frontmatter is not a dict literal this can locate (it
    /// may be computed, or imported), which leaves the diagnostic snippet-less
    /// rather than underlining an arbitrary offset.
    fn span(&self, key: Option<&str>) -> Option<SourceSpan> {
        let binding = Self::binding(self.source.root())?;
        let node = match key {
            None => binding.to_untyped(),
            Some(key) => {
                let Expr::Dict(dict) = binding.init()? else {
                    return None;
                };
                dict.items().find_map(|item| match item {
                    DictItem::Named(named) if named.name().get() == key => {
                        Some(named.expr().to_untyped())
                    }
                    _ => None,
                })?
            }
        };
        let range = self.source.find(node.span())?.range();
        Some(SourceSpan::new(range.start.into(), range.len()))
    }

    /// The `#let frontmatter = ..` binding anywhere in the tree, so a page that
    /// declares it inside a code block is located just as well as the
    /// conventional top-level form.
    fn binding(node: &SyntaxNode) -> Option<LetBinding<'_>> {
        if let Some(binding) = node.cast::<LetBinding>()
            && binding
                .kind()
                .bindings()
                .iter()
                .any(|ident| ident.get() == "frontmatter")
        {
            return Some(binding);
        }
        node.children().find_map(Self::binding)
    }
}

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
            .map(|&(_, ty, _)| ty)
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
    fn from_dict(dict: &Dict, origin: &Origin, config: &Config) -> Result<Self> {
        // Before reading, not after: a schema violation on a built-in key would
        // otherwise surface as whatever the built-in reader made of the value,
        // which says nothing about the collection that asked for it.
        Self::validate(dict, origin, config)?;
        let path = origin.path;
        let taxonomies: Vec<&str> = config
            .content
            .taxonomies
            .iter()
            .map(|(_, t)| t.key.as_str())
            .collect();
        let mut fm = Self::default();
        for (key, val) in dict {
            let key = key.as_str();
            match FIELDS.iter().find(|(name, ..)| *name == key) {
                Some((.., parse)) => parse(&mut fm, val, path, key)?,
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

    /// Hold the declared dict to the schema of the collection the page belongs
    /// to. A collection declaring none constrains nothing, which is the default
    /// and the whole of the previous behaviour.
    fn validate(dict: &Dict, origin: &Origin, config: &Config) -> Result<()> {
        for (key, field) in config.schema(origin.collection) {
            let error = match dict.get(key.as_str()) {
                Err(_) if field.optional => continue,
                Err(_) => SchemaError::missing(
                    origin.path,
                    origin.source.text(),
                    origin.span(None),
                    origin.collection,
                    key,
                    field.ty,
                ),
                Ok(value) if value.fits(field.ty) => continue,
                Ok(value) => SchemaError::mismatch(
                    origin.path,
                    origin.source.text(),
                    origin.span(Some(key)),
                    origin.collection,
                    key,
                    field.ty,
                    value.kind(),
                ),
            };
            return Err(error.into());
        }
        Ok(())
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

/// Typed accessors over an evaluated frontmatter [`Value`]. The `path`/`key`
/// parameters let a type mismatch name the file and field instead of being
/// silently dropped. [`ValueExt::str`] (infallible, for `extra` reads) is the
/// exception: a non-string there is simply "absent".
trait ValueExt {
    fn str(&self) -> Option<String>;
    fn string(&self, path: &Path, key: &str) -> Result<String>;
    fn boolean(&self, path: &Path, key: &str) -> Result<bool>;
    fn integer(&self, path: &Path, key: &str) -> Result<i64>;
    fn date(&self, path: &Path, key: &str) -> Result<time::Date>;
    fn strings(&self, path: &Path, key: &str) -> Result<Vec<String>>;
    /// Whether this value has the shape a collection's schema declared. The
    /// read-only counterpart of the accessors above: they parse one known key,
    /// this judges any key against a configured type.
    fn fits(&self, ty: FieldType) -> bool;
    /// This value's typst type name, for error messages.
    fn kind(&self) -> &'static str;
}

impl ValueExt for Value {
    fn str(&self) -> Option<String> {
        match self {
            Self::Str(s) => Some(s.to_string()),
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
            Self::Bool(b) => Ok(*b),
            _ => Err(
                ContentError::frontmatter_field(path, key, "a boolean", self.kind(), None).into(),
            ),
        }
    }

    fn integer(&self, path: &Path, key: &str) -> Result<i64> {
        match self {
            Self::Int(i) => Ok(*i),
            _ => Err(
                ContentError::frontmatter_field(path, key, "an integer", self.kind(), None).into(),
            ),
        }
    }

    fn date(&self, path: &Path, key: &str) -> Result<time::Date> {
        match self {
            Self::Datetime(Datetime::Date(d)) => Ok(*d),
            Self::Datetime(Datetime::Datetime(dt)) => Ok(dt.date()),
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
        // a wrong-typed *element* is an error too, never silently dropped,
        // same as every scalar accessor here.
        let wrong = |kind| {
            ContentError::frontmatter_field(path, key, "a list of strings", kind, None).into()
        };
        match self {
            Self::Array(arr) => arr
                .iter()
                .map(|v| v.str().ok_or_else(|| wrong(v.kind())))
                .collect(),
            _ => Err(wrong(self.kind())),
        }
    }

    fn fits(&self, ty: FieldType) -> bool {
        match ty {
            FieldType::Any => true,
            FieldType::Str => matches!(self, Self::Str(_)),
            FieldType::Bool => matches!(self, Self::Bool(_)),
            FieldType::Int => matches!(self, Self::Int(_)),
            FieldType::Float => matches!(self, Self::Float(_)),
            // The same two datetime shapes `date` reads: a time of day alone
            // is not a date, and would be dropped rather than ordered.
            FieldType::Date => matches!(
                self,
                Self::Datetime(Datetime::Date(_) | Datetime::Datetime(_))
            ),
            // Element-wise, like `strings`: an array holding one integer is not
            // a list of strings, and would fail the moment anything read it.
            FieldType::List => {
                matches!(self, Self::Array(items) if items.iter().all(|v| matches!(v, Self::Str(_))))
            }
        }
    }

    fn kind(&self) -> &'static str {
        self.ty().long_name()
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldType, Origin, ValueExt};
    use typst::foundations::{Str, Value};
    use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};

    fn page(text: &str) -> Source {
        let path = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("page.typ").expect("a valid vpath"),
        );
        Source::new(FileId::unique(path), text.into())
    }

    fn origin(source: &Source) -> Origin<'_> {
        Origin::new(source, std::path::Path::new("page.typ"), "blog")
    }

    /// The span is what makes a schema failure readable, and it is the one part
    /// the build never exercises on a green run: a locator that silently
    /// returned `None` would leave every one of these diagnostics snippet-less.
    #[test]
    fn a_key_locates_its_own_value_and_a_missing_one_the_binding() {
        let text = "#let frontmatter = (\n  title: \"Hello\",\n  hero: 3,\n)\n\nBody.\n";
        let source = page(text);
        let origin = origin(&source);

        let hero = origin.span(Some("hero")).expect("hero has a value");
        assert_eq!(&text[hero.offset()..hero.offset() + hero.len()], "3");
        let title = origin.span(Some("title")).expect("title has a value");
        assert_eq!(
            &text[title.offset()..title.offset() + title.len()],
            "\"Hello\""
        );
        // A key that is not there points at the binding, which is the thing
        // that should have carried it.
        // The binding node starts at `let`: in markup the `#` is a token of
        // its own, ahead of the expression.
        let binding = origin.span(None).expect("the binding");
        assert!(text[binding.offset()..].starts_with("let frontmatter"));
        assert_eq!(origin.span(Some("absent")), None);
    }

    /// A binding the locator cannot read leaves the diagnostic snippet-less
    /// rather than underlining an arbitrary offset.
    #[test]
    fn a_frontmatter_that_is_not_a_dict_literal_locates_nothing() {
        let imported = page("#import \"meta.typ\": frontmatter\n");
        assert_eq!(origin(&imported).span(None), None);

        let computed = page("#let frontmatter = build()\n");
        // The binding is still where it is; only the key inside it is not.
        assert!(origin(&computed).span(None).is_some());
        assert_eq!(origin(&computed).span(Some("title")), None);
    }

    #[test]
    fn a_list_fits_only_when_every_element_is_a_string() {
        let list = |items: Vec<Value>| Value::Array(items.into_iter().collect());
        let text = |s: &str| Value::Str(Str::from(s));
        assert!(list(vec![text("a"), text("b")]).fits(FieldType::List));
        assert!(list(vec![]).fits(FieldType::List));
        assert!(!list(vec![text("a"), Value::Int(2)]).fits(FieldType::List));
        assert!(!text("a").fits(FieldType::List));
        // `any` is presence alone, so every one of them satisfies it.
        assert!(Value::Int(2).fits(FieldType::Any));
        assert!(!Value::Int(2).fits(FieldType::Str));
    }
}
