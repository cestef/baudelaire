use crate::config::PermalinkCtx;
use std::collections::BTreeMap;
use std::path::Path;

use miette::SourceSpan;
use serde::{Deserialize, Serialize};
use typst::foundations::{Datetime, Dict, Module, Value};
use typst::syntax::{
    Source, SyntaxNode,
    ast::{ArrayItem, AstNode, DictItem, Expr, LetBinding, Markup},
};

use crate::codegen;
use crate::config::dispatch::Keys;
use crate::config::{Config, FieldSchema, FieldType};
use crate::error::{ContentError, Result, SchemaError};

/// A frontmatter field parser: reads the evaluated value into its slot on `fm`,
/// naming `path`/`key` on a type mismatch (never silently dropped).
type Field = fn(fm: &mut Frontmatter, value: &Value, path: &Path, key: &str) -> Result<()>;

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
        |fm, v, p, k| {
            fm.title = Some(v.string(p, k)?);
            Ok(())
        },
    ),
    (
        "date",
        || FieldType::Date,
        |fm, v, p, k| {
            fm.date = Some(v.date(p, k)?);
            Ok(())
        },
    ),
    (
        "updated",
        || FieldType::Date,
        |fm, v, p, k| {
            fm.updated = Some(v.date(p, k)?);
            Ok(())
        },
    ),
    (
        "expiry",
        || FieldType::Date,
        |fm, v, p, k| {
            fm.expiry = Some(v.date(p, k)?);
            Ok(())
        },
    ),
    (
        "draft",
        || FieldType::Bool,
        |fm, v, p, k| {
            fm.draft = v.boolean(p, k)?;
            Ok(())
        },
    ),
    (
        "slug",
        || FieldType::Str,
        |fm, v, p, k| {
            fm.slug = Some(v.string(p, k)?);
            Ok(())
        },
    ),
    (
        "path",
        || FieldType::Str,
        |fm, v, p, k| {
            fm.path = Some(v.string(p, k)?);
            Ok(())
        },
    ),
    (
        "lang",
        || FieldType::Str,
        |fm, v, p, k| {
            fm.lang = Some(v.string(p, k)?);
            Ok(())
        },
    ),
    (
        "translation",
        || FieldType::Str,
        |fm, v, p, k| {
            fm.translation = Some(v.string(p, k)?);
            Ok(())
        },
    ),
    (
        "template",
        || FieldType::Str,
        |fm, v, p, k| {
            fm.template = Some(v.string(p, k)?);
            Ok(())
        },
    ),
    (
        "order",
        || FieldType::Int,
        |fm, v, p, k| {
            fm.order = Some(v.integer(p, k)?);
            Ok(())
        },
    ),
    (
        "redirect",
        || FieldType::List(Box::new(FieldType::Str)),
        |fm, v, p, k| {
            fm.redirect = v.strings(p, k)?;
            Ok(())
        },
    ),
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

    /// The byte span of the value `path` leads to inside
    /// `#let frontmatter = (..)`, or of the binding itself for the empty path: a
    /// field that is absent has nowhere of its own to point at.
    ///
    /// Walks as far down `path` as the source literally spells, and underlines
    /// the deepest value it reached. A nested key the page never wrote stops one
    /// step short, at the dictionary that should have held it, which is where a
    /// reader would go to add it.
    ///
    /// `None` when the frontmatter is not a dict literal this can locate (it
    /// may be computed, or imported), which leaves the diagnostic snippet-less
    /// rather than underlining an arbitrary offset.
    fn span(&self, path: &[Step]) -> Option<SourceSpan> {
        let binding = Self::binding(self.source.root())?;
        let mut node = binding.to_untyped();
        let mut reached = 0;
        if let Some(mut expr) = binding.init() {
            for step in path {
                let Some(next) = Self::descend(expr, step) else {
                    break;
                };
                node = next.to_untyped();
                expr = next;
                reached += 1;
            }
        }
        // A path that got nowhere is a frontmatter this cannot read into at all
        // (computed, or imported): underlining the binding would label the whole
        // of it as the value that is wrong.
        if !path.is_empty() && reached == 0 {
            return None;
        }
        let range = self.source.find(node.span())?.range();
        Some(SourceSpan::new(range.start.into(), range.len()))
    }

    /// One step into a literal: a named dict item, or a positional array
    /// element. Anything else (a spread, a computed key, a call) is not a
    /// literal this can point inside of, and stops the walk.
    fn descend<'b>(expr: Expr<'b>, step: &Step) -> Option<Expr<'b>> {
        match (expr, step) {
            (Expr::Dict(dict), Step::Key(key)) => dict.items().find_map(|item| match item {
                DictItem::Named(named) if named.name().get() == key => Some(named.expr()),
                _ => None,
            }),
            (Expr::Array(array), Step::Index(i)) => match array.items().nth(*i)? {
                ArrayItem::Pos(expr) => Some(expr),
                ArrayItem::Spread(_) => None,
            },
            _ => None,
        }
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
        let Some(fault) = Check::default().dict(config.schema(origin.collection), dict) else {
            return Ok(());
        };
        let (source, key) = (origin.source.text(), fault.key());
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
    /// This value's typst type name, for error messages.
    fn kind(&self) -> &'static str;
}

/// One step from the frontmatter dict down to the value a diagnostic is about:
/// a key of a dictionary, or the position of a list element.
///
/// What both halves of a nested schema failure are built from: the dotted name
/// the message uses (`authors.1.email`) and the walk that finds its span in the
/// page source.
#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Index(usize),
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Key(key) => f.write_str(key),
            Self::Index(i) => write!(f, "{i}"),
        }
    }
}

/// The first way a frontmatter value failed the type declared for it, and where.
///
/// The check stops at the first fault, so a page fixes one thing at a time
/// rather than reading a list of consequences of the same mistake.
#[derive(Debug)]
enum Fault {
    Missing {
        path: Vec<Step>,
        want: FieldType,
    },
    Mismatch {
        path: Vec<Step>,
        want: FieldType,
        got: &'static str,
    },
}

impl Fault {
    /// The steps to the value at fault.
    fn path(&self) -> &[Step] {
        let (Self::Missing { path, .. } | Self::Mismatch { path, .. }) = self;
        path
    }

    /// The steps to whatever holds it: where a missing field would go.
    fn parent(&self) -> &[Step] {
        self.path().split_last().map_or(&[], |(_, rest)| rest)
    }

    /// How a diagnostic names the field: dotted, so a nested one is located
    /// without the message having to describe the nesting.
    fn key(&self) -> String {
        self.path()
            .iter()
            .map(Step::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// A schema check in progress: the path walked so far, so a fault deep inside a
/// dictionary names the field it happened at rather than the top-level one it
/// happened under.
#[derive(Default)]
struct Check {
    path: Vec<Step>,
}

impl Check {
    /// Every field a schema declares, against the dictionary that should carry
    /// them. Keys the schema does not name are not the schema's business, so
    /// extra frontmatter passes through as it always has.
    fn dict(&mut self, schema: &[(String, FieldSchema)], dict: &Dict) -> Option<Fault> {
        for (key, field) in schema {
            self.path.push(Step::Key(key.clone()));
            let fault = match dict.get(key.as_str()) {
                Err(_) if field.optional => None,
                Err(_) => Some(Fault::Missing {
                    path: self.path.clone(),
                    want: field.ty.clone(),
                }),
                Ok(value) => self.value(&field.ty, value),
            };
            self.path.pop();
            if fault.is_some() {
                return fault;
            }
        }
        None
    }

    /// One value against one type. Compound types recurse, so the fault names
    /// the element or the nested key that broke rather than the outermost value
    /// containing it.
    fn value(&mut self, ty: &FieldType, value: &Value) -> Option<Fault> {
        let fits = match (ty, value) {
            // Element-wise: an array holding one integer is not a list of
            // strings, and would fail the moment anything read it.
            (FieldType::List(inner), Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    self.path.push(Step::Index(i));
                    let fault = self.value(inner, item);
                    self.path.pop();
                    if fault.is_some() {
                        return fault;
                    }
                }
                true
            }
            (FieldType::Dict(fields), Value::Dict(nested)) => return self.dict(fields, nested),
            // Everything a type expression can end in, and the compound types
            // whose value was not the shape the two arms above match.
            _ => Self::scalar(ty, value),
        };
        (!fits).then(|| Fault::Mismatch {
            path: self.path.clone(),
            want: ty.clone(),
            got: value.kind(),
        })
    }

    /// Whether a value is the leaf a type asks for. A compound type reaching
    /// here was already handed a value of the wrong shape.
    fn scalar(ty: &FieldType, value: &Value) -> bool {
        match ty {
            FieldType::Any => true,
            FieldType::Str => matches!(value, Value::Str(_)),
            FieldType::Bool => matches!(value, Value::Bool(_)),
            FieldType::Int => matches!(value, Value::Int(_)),
            FieldType::Float => matches!(value, Value::Float(_)),
            // The same two datetime shapes `date` reads: a time of day alone
            // is not a date, and would be dropped rather than ordered.
            FieldType::Date => matches!(
                value,
                Value::Datetime(Datetime::Date(_) | Datetime::Datetime(_))
            ),
            FieldType::List(_) | FieldType::Dict(_) => false,
        }
    }
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
            // A date written as the ISO day it renders as, rather than as the
            // call. One reader, so every dialect a page may be written in reads
            // a date the same way.
            Self::Str(text) => text
                .as_str()
                .parse::<crate::content::date::Iso>()
                .map(|iso| iso.0)
                .map_err(|()| {
                    ContentError::frontmatter_field(
                        path,
                        key,
                        "a date",
                        "a string that is not an ISO day",
                        Some(
                            "write it as `\"2024-01-01\"`, or as `datetime(year: 2024, month: 1, day: 1)`",
                        ),
                    )
                    .into()
                }),
            _ => Err(ContentError::frontmatter_field(
                path,
                key,
                "a date",
                self.kind(),
                Some("write dates as `\"2024-01-01\"` or `datetime(year: 2024, month: 1, day: 1)`"),
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

    fn kind(&self) -> &'static str {
        self.ty().long_name()
    }
}

#[cfg(test)]
mod tests {
    use super::{Check, FieldSchema, FieldType, Origin, Step};
    use typst::foundations::{Dict, Str, Value};
    use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};

    fn key(name: &str) -> Step {
        Step::Key(name.to_owned())
    }

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

        let hero = origin.span(&[key("hero")]).expect("hero has a value");
        assert_eq!(&text[hero.offset()..hero.offset() + hero.len()], "3");
        let title = origin.span(&[key("title")]).expect("title has a value");
        assert_eq!(
            &text[title.offset()..title.offset() + title.len()],
            "\"Hello\""
        );
        // A key that is not there points at the binding, which is the thing
        // that should have carried it.
        // The binding node starts at `let`: in markup the `#` is a token of
        // its own, ahead of the expression.
        let binding = origin.span(&[]).expect("the binding");
        assert!(text[binding.offset()..].starts_with("let frontmatter"));
        assert_eq!(origin.span(&[key("absent")]), None);
    }

    /// A nested path stops at the deepest value the page actually wrote, which
    /// is where the field it is missing would go.
    #[test]
    fn a_nested_key_locates_the_value_or_the_dict_that_should_hold_it() {
        let text = "#let frontmatter = (\n  authors: ((name: \"A\"), (name: 2)),\n)\n";
        let source = page(text);
        let origin = origin(&source);

        let path = [key("authors"), Step::Index(1), key("name")];
        let name = origin.span(&path).expect("the second author's name");
        assert_eq!(&text[name.offset()..name.offset() + name.len()], "2");

        let absent = [key("authors"), Step::Index(0), key("email")];
        let dict = origin.span(&absent).expect("the dict that should hold it");
        assert_eq!(
            &text[dict.offset()..dict.offset() + dict.len()],
            "(name: \"A\")"
        );
    }

    /// A binding the locator cannot read leaves the diagnostic snippet-less
    /// rather than underlining an arbitrary offset.
    #[test]
    fn a_frontmatter_that_is_not_a_dict_literal_locates_nothing() {
        let imported = page("#import \"meta.typ\": frontmatter\n");
        assert_eq!(origin(&imported).span(&[]), None);

        let computed = page("#let frontmatter = build()\n");
        // The binding is still where it is; only the key inside it is not.
        assert!(origin(&computed).span(&[]).is_some());
        assert_eq!(origin(&computed).span(&[key("title")]), None);
    }

    fn required(ty: FieldType) -> FieldSchema {
        FieldSchema {
            ty,
            optional: false,
        }
    }

    fn list(items: Vec<Value>) -> Value {
        Value::Array(items.into_iter().collect())
    }

    fn text(s: &str) -> Value {
        Value::Str(Str::from(s))
    }

    fn dict(items: Vec<(&str, Value)>) -> Value {
        Value::Dict(
            items
                .into_iter()
                .map(|(k, v)| (Str::from(k), v))
                .collect::<Dict>(),
        )
    }

    /// Whether a value satisfies a type, which is what every schema check
    /// reduces to once the path bookkeeping is stripped away.
    fn fits(ty: &FieldType, value: &Value) -> bool {
        Check::default().value(ty, value).is_none()
    }

    #[test]
    fn a_list_fits_only_when_every_element_has_the_declared_type() {
        let strings = FieldType::parse("list").expect("a valid type");
        assert!(fits(&strings, &list(vec![text("a"), text("b")])));
        assert!(fits(&strings, &list(vec![])));
        assert!(!fits(&strings, &list(vec![text("a"), Value::Int(2)])));
        assert!(!fits(&strings, &text("a")));

        let ints = FieldType::parse("list<int>").expect("a valid type");
        assert!(fits(&ints, &list(vec![Value::Int(1), Value::Int(2)])));
        assert!(!fits(&ints, &list(vec![Value::Int(1), text("a")])));

        let nested = FieldType::parse("list<list<int>>").expect("a valid type");
        assert!(fits(&nested, &list(vec![list(vec![Value::Int(1)])])));
        assert!(!fits(&nested, &list(vec![Value::Int(1)])));

        // `any` is presence alone, so every one of them satisfies it.
        assert!(fits(&FieldType::Any, &Value::Int(2)));
        assert!(!fits(&FieldType::Str, &Value::Int(2)));
    }

    #[test]
    fn a_dict_fits_when_the_fields_it_declares_do() {
        let mut ty = FieldType::parse("list<dict>").expect("a valid type");
        *ty.fields_mut().expect("a dict leaf") = vec![
            ("name".to_owned(), required(FieldType::Str)),
            (
                "age".to_owned(),
                FieldSchema {
                    ty: FieldType::Int,
                    optional: true,
                },
            ),
        ];

        assert!(fits(&ty, &list(vec![dict(vec![("name", text("A"))])])));
        // an undeclared key is not the schema's business
        assert!(fits(
            &ty,
            &list(vec![dict(vec![("name", text("A")), ("bio", text("B"))])])
        ));
        // ..but a declared one is, present or absent
        assert!(!fits(&ty, &list(vec![dict(vec![("age", Value::Int(3))])])));
        assert!(!fits(
            &ty,
            &list(vec![dict(vec![("name", text("A")), ("age", text("3"))])])
        ));
        assert!(!fits(&ty, &list(vec![text("A")])));
    }

    /// The fault names the field that broke, not the top-level one it sits
    /// under: a page with fifty authors is told which.
    #[test]
    fn a_fault_names_the_nested_field_it_happened_at() {
        let mut ty = FieldType::parse("list<dict>").expect("a valid type");
        *ty.fields_mut().expect("a dict leaf") =
            vec![("email".to_owned(), required(FieldType::Str))];
        let schema = vec![("authors".to_owned(), required(ty))];
        let value = list(vec![
            dict(vec![("email", text("a@example.com"))]),
            dict(vec![]),
        ]);
        let Value::Dict(frontmatter) = dict(vec![("authors", value)]) else {
            unreachable!("built as a dict")
        };

        let fault = Check::default()
            .dict(&schema, &frontmatter)
            .expect("the second author declares no email");
        assert_eq!(fault.key(), "authors.1.email");
    }
}
