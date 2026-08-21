//! Typst source generation binding a page body to a layout template: the
//! synthetic module imports the template function and applies it document-wide
//! via a show rule, so the template itself produces the DOM.

use std::fmt;
use std::path::Path;

use crate::codegen::{Import, Str, Typst, Value};
use crate::content::Frontmatter;

/// Where a page's frontmatter dict comes from, as an expression in the
/// synthetic module.
pub(in crate::engine) enum Bind {
    /// Import the page module's own `frontmatter` export.
    Import,
    /// A dict literal, for a page with no export.
    Literal(String),
}

impl Bind {
    /// The expression the `frontmatter` key holds, written after the import
    /// line [`Bind::Import`] needs.
    fn expr(&self, f: &mut fmt::Formatter<'_>, page: &str) -> Result<Value, fmt::Error> {
        match self {
            Self::Import => {
                let import = Import::new(page, Frontmatter::EXPORT, Frontmatter::ALIAS);
                writeln!(f, "{import}")?;
                Ok(Value::Raw(Frontmatter::ALIAS.to_owned()))
            }
            Self::Literal(dict) => Ok(Value::Raw(dict.clone())),
        }
    }
}

/// The page content the template wraps.
pub(in crate::engine) enum Body<'a> {
    /// `#include` the page's own file.
    Include,
    /// Generated markup, for a listing that has no file.
    Inline(&'a str),
}

/// The data a layout template receives as its first argument: the
/// `(frontmatter:, taxonomies:, nav:, lang:, ..)` dict passed to the template
/// function.
///
/// Nothing *site-wide* may enter it, since the wrapper text is the page's cache
/// fingerprint and would then tie every page's identity to every other.
pub(in crate::engine) struct Context {
    pub data: Bind,
    /// The page's terms, keyed by taxonomy: `(tags: ("a", "b"))`.
    pub taxonomies: Value,
    /// Prev/next sibling links: `(prev: (url: .., title: ..), next: none)`.
    pub nav: Value,
    pub lang: Value,
    /// The page's editions in every language, itself included. Empty on a
    /// single-language site.
    pub translations: Value,
    /// The current language's UI-string table.
    pub strings: Value,
    /// The page's reading estimate, `(words: 1200, minutes: 6)`.
    pub reading: Value,
    /// The pages whose content links to this one, empty unless
    /// `links { backlinks }` is on.
    ///
    /// The one entry that is *not* part of the page's cache fingerprint: the
    /// graph it comes from does not exist until every page has rendered.
    pub backlinks: Value,
    pub url: Value,
    pub collection: Value,
    /// The files sitting beside a page bundle, as authored name to served URL.
    /// Empty for a page that shares its directory with its neighbours.
    pub assets: Value,
    /// Who the page credits, keyed by role.
    pub credits: Value,
    /// The pages that name this one as an entity, as the rows a listing
    /// carries. Empty for a page nothing names.
    pub members: Value,
    /// The page's date in both forms, `(iso: .., display: ..)`, or `none` when
    /// the page carries no date; typst's `datetime.display` knows English month
    /// names only, so a template cannot localize `frontmatter.date` itself.
    pub date: Value,
    /// The page's own file, project-root-absolute
    /// (`/content/posts/hello.typ`), or `none` for a generated listing, whose
    /// path names no file anyone can open.
    pub source: Value,
    /// What git knows about the page's own file, or `none` where nothing does:
    /// `content { history }` off, a generated listing, an uncommitted page.
    pub git: Value,
}

impl Context {
    /// The `page` dict a template is applied to, with `frontmatter` already
    /// spelled as whatever expression holds it.
    pub(in crate::engine) fn dict(&self, frontmatter: Value) -> Value {
        let Self {
            data: _,
            taxonomies,
            credits,
            members,
            nav,
            lang,
            translations,
            strings,
            reading,
            backlinks,
            date,
            url,
            collection,
            assets,
            source,
            git,
        } = self;
        Value::dict([
            ("frontmatter", frontmatter),
            ("taxonomies", taxonomies.clone()),
            ("credits", credits.clone()),
            ("members", members.clone()),
            ("nav", nav.clone()),
            ("lang", lang.clone()),
            ("translations", translations.clone()),
            ("strings", strings.clone()),
            ("reading", reading.clone()),
            ("backlinks", backlinks.clone()),
            ("date", date.clone()),
            ("url", url.clone()),
            ("collection", collection.clone()),
            ("assets", assets.clone()),
            ("source", source.clone()),
            ("git", git.clone()),
        ])
    }
}

/// A synthetic typst module applying a layout template to a page, rendered to
/// compilable typst source by [`fmt::Display`].
pub(in crate::engine) struct Layout<'a> {
    /// The import root the template is loaded from: `/templates` for a project
    /// file, or a theme's package spec plus its template directory
    /// (`@preview/plume:1.0/templates`).
    dir: &'a str,
    /// Template file within `dir` (e.g. `post.typ`).
    file: &'a str,
    /// Project-root-absolute virtual path of the page (`/content/posts/a.typ`);
    /// what [`Bind::Import`] and [`Body::Include`] resolve against.
    page: &'a str,
    context: Context,
    body: Body<'a>,
}

impl<'a> Layout<'a> {
    pub(in crate::engine) fn new(
        dir: &'a str,
        file: &'a str,
        page: &'a str,
        context: Context,
        body: Body<'a>,
    ) -> Self {
        Self {
            dir,
            file,
            page,
            context,
            body,
        }
    }

    /// The template function to apply: the file stem (`post.typ` -> `post`).
    fn func(&self) -> &str {
        Path::new(self.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main")
    }

    /// The full import path of the template (`/templates/post.typ`).
    fn import(&self) -> String {
        format!("{}/{}", self.dir, self.file)
    }
}

impl fmt::Display for Layout<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{}",
            Import::new(&self.import(), self.func(), "__layout")
        )?;
        let frontmatter = self.context.data.expr(f, self.page)?;
        let page = self.context.dict(frontmatter);
        writeln!(f, "#show: __body => __layout({}, __body)", Typst(&page))?;
        match &self.body {
            Body::Include => write!(f, "#include {}", Str(self.page)),
            Body::Inline(markup) => f.write_str(markup),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(source: &str) -> Value {
        Value::Raw(source.to_owned())
    }

    /// A context holding one spelling of every value, so a test names only what
    /// it is about.
    fn context(data: Bind) -> Context {
        Context {
            data,
            taxonomies: raw("(:)"),
            credits: raw("(:)"),
            members: raw("()"),
            nav: raw("(prev: none, next: none)"),
            lang: Value::str("en"),
            translations: raw("()"),
            strings: raw("(:)"),
            reading: raw("(words: 0, minutes: 0)"),
            backlinks: raw("()"),
            date: Value::None,
            url: Value::str("/posts/a/"),
            collection: Value::str("posts"),
            assets: raw("(:)"),
            source: Value::str("/content/posts/a.typ"),
            git: Value::None,
        }
    }

    /// The dict as every assertion below reads it, with `frontmatter` spelled
    /// as the test's own binding.
    fn dict(frontmatter: &str) -> String {
        format!(
            "(frontmatter: {frontmatter}, taxonomies: (:), credits: (:), members: (), \
             nav: (prev: none, next: none), lang: \"en\", translations: (), strings: (:), \
             reading: (words: 0, minutes: 0), backlinks: (), date: none, url: \"/posts/a/\", \
             collection: \"posts\", assets: (:), source: \"/content/posts/a.typ\", \
             git: none)"
        )
    }

    #[test]
    fn imports_the_export_and_includes_the_page() {
        let out = Layout::new(
            "/templates",
            "post.typ",
            "/content/posts/a.typ",
            Context {
                taxonomies: raw("(tags: (\"a\",))"),
                ..context(Bind::Import)
            },
            Body::Include,
        )
        .to_string();
        assert_eq!(
            out,
            format!(
                "#import \"/templates/post.typ\": post as __layout\n\
                 #import \"/content/posts/a.typ\": frontmatter as __data\n\
                 #show: __body => __layout({}, __body)\n\
                 #include \"/content/posts/a.typ\"",
                dict("__data").replace("taxonomies: (:)", "taxonomies: (tags: (\"a\",))")
            )
        );
    }

    #[test]
    fn inlines_generated_listings() {
        let out = Layout::new(
            "/templates",
            "list.typ",
            "/content/tags/x.typ",
            context(Bind::Literal("(title: \"X\")".to_owned())),
            Body::Inline("listing body"),
        )
        .to_string();
        assert_eq!(
            out,
            format!(
                "#import \"/templates/list.typ\": list as __layout\n\
                 #show: __body => __layout({}, __body)\n\
                 listing body",
                dict("(title: \"X\")")
            )
        );
    }

    #[test]
    fn escapes_paths_that_would_break_the_literal() {
        let out = Layout::new(
            "/a\"b",
            "x.typ",
            "/content/x.typ",
            context(Bind::Literal("(:)".to_owned())),
            Body::Include,
        )
        .to_string();
        assert!(out.starts_with("#import \"/a\\\"b/x.typ\": x as __layout\n"));
    }

    #[test]
    fn template_named_page_is_not_shadowed() {
        let out = Layout::new(
            "/templates",
            "page.typ",
            "/content/b.typ",
            context(Bind::Literal("(t: 1)".to_owned())),
            Body::Inline("b"),
        )
        .to_string();
        assert!(out.contains(": page as __layout"), "{out}");
        assert!(
            out.contains(&format!("__layout({}, __body)", dict("(t: 1)"))),
            "{out}"
        );
    }
}
