//! Typst source generation binding a page body to a layout template.
//!
//! This is typst *code* generation, not HTML templating: the synthetic module
//! imports the template function and applies it document-wide via a show rule,
//! so the template itself produces the DOM. No HTML is ever assembled as text.

use std::fmt;
use std::path::Path;

use crate::codegen::Str;

/// A synthetic typst module that wraps a page's body in its layout template,
/// passing the page's frontmatter as data. Renders (via [`fmt::Display`]) to
/// compilable typst source.
pub(super) struct Layout<'a> {
    /// Template directory relative to the project root (e.g. `templates`).
    dir: &'a Path,
    /// Template file within `dir` (e.g. `post.typ`).
    file: &'a str,
    /// Frontmatter dict literal, e.g. `(title: "x")`.
    data: &'a str,
    /// Parsed taxonomies as a dict literal, e.g. `(tags: ("a", "b"))` — passed
    /// alongside the raw frontmatter so a template reads a page's taxonomy terms
    /// the same structured way a listing reads an entry's, not by guessing which
    /// frontmatter keys are taxonomies.
    taxonomies: &'a str,
    /// Page body markup.
    body: &'a str,
}

impl<'a> Layout<'a> {
    pub(super) fn new(
        dir: &'a Path,
        file: &'a str,
        data: &'a str,
        taxonomies: &'a str,
        body: &'a str,
    ) -> Self {
        Self {
            dir,
            file,
            data,
            taxonomies,
            body,
        }
    }

    /// The template function to apply: the file stem (`post.typ` → `post`).
    fn func(&self) -> &str {
        Path::new(self.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main")
    }

    /// The project-root-absolute import path (`/templates/post.typ`).
    fn import(&self) -> String {
        format!("/{}/{}", self.dir.display(), self.file)
    }
}

impl fmt::Display for Layout<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Import under an internal alias and pass the data inline, so the
        // template function is never shadowed by a `page`/`body` binding — even
        // when the template file is itself named `page.typ` or `body.typ`.
        writeln!(f, "#import {}: {} as __layout", Str(&self.import()), self.func())?;
        writeln!(
            f,
            "#show: __body => __layout((frontmatter: {}, taxonomies: {}), __body)",
            self.data, self.taxonomies
        )?;
        f.write_str(self.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_body_to_template_function() {
        let out = Layout::new(
            Path::new("templates"),
            "post.typ",
            "(title: \"Hi\")",
            "(tags: (\"a\",))",
            "body text",
        )
        .to_string();
        assert_eq!(
            out,
            "#import \"/templates/post.typ\": post as __layout\n\
             #show: __body => __layout((frontmatter: (title: \"Hi\"), taxonomies: (tags: (\"a\",))), __body)\n\
             body text"
        );
    }

    #[test]
    fn escapes_paths_that_would_break_the_literal() {
        let out = Layout::new(Path::new("a\"b"), "x.typ", "()", "(:)", "").to_string();
        assert!(out.starts_with("#import \"/a\\\"b/x.typ\": x as __layout\n"));
    }

    #[test]
    fn template_named_page_is_not_shadowed() {
        // Regression: a `page.typ` template must still be callable even though
        // pages carry a `page` dict; the alias makes this collision-proof.
        let out = Layout::new(Path::new("templates"), "page.typ", "(t: 1)", "(:)", "b").to_string();
        assert!(out.contains(": page as __layout"), "{out}");
        assert!(out.contains("__layout((frontmatter: (t: 1), taxonomies: (:)), __body)"), "{out}");
    }
}
