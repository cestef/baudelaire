//! Typst source generation binding a page body to a layout template.
//!
//! This is typst *code* generation, not HTML templating: the synthetic module
//! imports the template function and applies it document-wide via a show rule,
//! so the template itself produces the DOM. No HTML is ever assembled as text,
//! and a real page's source is never touched either: its frontmatter arrives as
//! a module *import* of the page's own `frontmatter` export, and its body via
//! `#include`. Only generated listings (which have no file) inline anything.

use std::fmt;
use std::path::Path;

use crate::codegen::Str;

/// Where a page's frontmatter dict comes from, as an expression in the
/// synthetic module.
pub(in crate::engine) enum Bind<'a> {
    /// Import the page module's own `frontmatter` export.
    Import,
    /// A dict literal: `(:)` for exportless pages, generated data for listings.
    Literal(&'a str),
}

/// The page content the template wraps.
pub(in crate::engine) enum Body<'a> {
    /// `#include` the real page file: its source compiles as authored.
    Include,
    /// Generated markup, inlined (listings have no file to include).
    Inline(&'a str),
}

/// The data a layout template receives as its first argument: the
/// `(frontmatter:, taxonomies:, nav:, sections:)` dict passed to the template
/// function. Grouping these keeps [`Layout`] to the few things that locate the
/// template and page, rather than one field per dict entry.
pub(in crate::engine) struct Context<'a> {
    /// Where the page's frontmatter dict comes from.
    pub data: Bind<'a>,
    /// Parsed taxonomies as a dict literal, e.g. `(tags: ("a", "b"))`, passed
    /// alongside the frontmatter so a template reads a page's taxonomy terms
    /// the same structured way a listing reads an entry's, not by guessing which
    /// frontmatter keys are taxonomies.
    pub taxonomies: &'a str,
    /// Prev/next sibling links as a dict literal, e.g.
    /// `(prev: (url: "..", title: ".."), next: none)`, exposed to the template as
    /// `page.nav` for older/newer navigation.
    pub nav: &'a str,
    /// The site's content collections as an array literal, exposed to the
    /// template as `page.sections`, the single source a site nav is built from.
    pub sections: &'a str,
    /// The page's language code, exposed as `page.lang`.
    pub lang: &'a str,
    /// The page's editions in every language as an array literal, e.g.
    /// `((lang: "en", url: "..", title: ".."), ..)`, exposed as
    /// `page.translations` for a language switcher. Empty on a single-language
    /// site. Part of the wrapper so adding, removing, or retitling a translation
    /// refingerprints the pages that show it.
    pub translations: &'a str,
    /// The current language's UI-string table as a dict literal, exposed as
    /// `page.strings`.
    pub strings: &'a str,
}

/// A synthetic typst module that applies a layout template to a page,
/// passing the page's frontmatter as data. Renders (via [`fmt::Display`]) to
/// compilable typst source.
pub(in crate::engine) struct Layout<'a> {
    /// The import root the template is loaded from: `/templates` for a project
    /// file, or a theme's package spec plus its template directory
    /// (`@preview/plume:1.0/templates`), which the compiler resolves itself.
    dir: &'a str,
    /// Template file within `dir` (e.g. `post.typ`).
    file: &'a str,
    /// Project-root-absolute virtual path of the page (`/content/posts/a.typ`);
    /// what [`Bind::Import`] and [`Body::Include`] resolve against.
    page: &'a str,
    /// The dict handed to the template function.
    context: Context<'a>,
    body: Body<'a>,
}

impl<'a> Layout<'a> {
    pub(in crate::engine) fn new(
        dir: &'a str,
        file: &'a str,
        page: &'a str,
        context: Context<'a>,
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
        // import under internal aliases so the template function and the data
        // are never shadowed by a page binding, even a `page.typ`/`body.typ`.
        writeln!(
            f,
            "#import {}: {} as __layout",
            Str(&self.import()),
            self.func()
        )?;
        let Context {
            data,
            taxonomies,
            nav,
            sections,
            lang,
            translations,
            strings,
        } = &self.context;
        let frontmatter: &dyn fmt::Display = match data {
            Bind::Import => {
                writeln!(f, "#import {}: frontmatter as __data", Str(self.page))?;
                &"__data"
            }
            Bind::Literal(dict) => dict,
        };
        writeln!(
            f,
            "#show: __body => __layout((frontmatter: {frontmatter}, taxonomies: {taxonomies}, nav: {nav}, sections: {sections}, lang: {}, translations: {translations}, strings: {strings}), __body)",
            Str(lang)
        )?;
        match &self.body {
            Body::Include => write!(f, "#include {}", Str(self.page)),
            Body::Inline(markup) => f.write_str(markup),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_the_export_and_includes_the_page() {
        let out = Layout::new(
            "/templates",
            "post.typ",
            "/content/posts/a.typ",
            Context {
                data: Bind::Import,
                taxonomies: "(tags: (\"a\",))",
                nav: "(prev: none, next: none)",
                sections: "()",
                lang: "en",
                translations: "()",
                strings: "(:)",
            },
            Body::Include,
        )
        .to_string();
        assert_eq!(
            out,
            "#import \"/templates/post.typ\": post as __layout\n\
             #import \"/content/posts/a.typ\": frontmatter as __data\n\
             #show: __body => __layout((frontmatter: __data, taxonomies: (tags: (\"a\",)), nav: (prev: none, next: none), sections: (), lang: \"en\", translations: (), strings: (:)), __body)\n\
             #include \"/content/posts/a.typ\""
        );
    }

    #[test]
    fn inlines_generated_listings() {
        let out = Layout::new(
            "/templates",
            "list.typ",
            "/content/tags/x.typ",
            Context {
                data: Bind::Literal("(title: \"X\")"),
                taxonomies: "(:)",
                nav: "(prev: none, next: none)",
                sections: "()",
                lang: "en",
                translations: "()",
                strings: "(:)",
            },
            Body::Inline("listing body"),
        )
        .to_string();
        assert_eq!(
            out,
            "#import \"/templates/list.typ\": list as __layout\n\
             #show: __body => __layout((frontmatter: (title: \"X\"), taxonomies: (:), nav: (prev: none, next: none), sections: (), lang: \"en\", translations: (), strings: (:)), __body)\n\
             listing body"
        );
    }

    #[test]
    fn escapes_paths_that_would_break_the_literal() {
        let out = Layout::new(
            "/a\"b",
            "x.typ",
            "/content/x.typ",
            Context {
                data: Bind::Literal("(:)"),
                taxonomies: "(:)",
                nav: "(prev: none, next: none)",
                sections: "()",
                lang: "en",
                translations: "()",
                strings: "(:)",
            },
            Body::Include,
        )
        .to_string();
        assert!(out.starts_with("#import \"/a\\\"b/x.typ\": x as __layout\n"));
    }

    #[test]
    fn template_named_page_is_not_shadowed() {
        // Regression: a `page.typ` template must still be callable even though
        // pages carry a `page` dict; the alias makes this collision-proof.
        let out = Layout::new(
            "/templates",
            "page.typ",
            "/content/b.typ",
            Context {
                data: Bind::Literal("(t: 1)"),
                taxonomies: "(:)",
                nav: "(prev: none, next: none)",
                sections: "()",
                lang: "en",
                translations: "()",
                strings: "(:)",
            },
            Body::Inline("b"),
        )
        .to_string();
        assert!(out.contains(": page as __layout"), "{out}");
        assert!(
            out.contains(
                "__layout((frontmatter: (t: 1), taxonomies: (:), nav: (prev: none, next: none), sections: (), lang: \"en\", translations: (), strings: (:)), __body)"
            ),
            "{out}"
        );
    }
}
