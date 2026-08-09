//! Layout files a build was told to use and could not find, and one it found
//! and could not use.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Code, markup};

/// A template the build was pointed at that is not in the template directory,
/// nor in the theme's. Fatal, and reported before the first compile: the
/// alternative is the compiler's own `file not found`, once per page, against
/// the generated wrapper that imports it rather than against the config line or
/// the frontmatter key that named it.
#[derive(Debug, Error, Diagnostic)]
#[error("template {} was not found", Code(.file))]
#[diagnostic(code(baudelaire::template::missing), help("{help}"))]
pub struct TemplateMissing {
    /// The filename as it was written, relative to the template directory.
    pub file: String,
    /// What asked for it: a config key, or the page whose frontmatter named it.
    /// Carries its own markup, both spellings being code.
    pub asked: String,
    /// Where it was looked for, and what to do about it. Built here, so it
    /// carries its own markup rather than being escaped as foreign text.
    pub help: String,
}

impl TemplateMissing {
    /// `asked` names the config key or the page, already marked up by its
    /// caller (a key and a path are both code); `searched` is the directories
    /// looked in, in the order they were looked in.
    pub fn new(file: &str, asked: &str, searched: &[String]) -> Self {
        let places = searched
            .iter()
            .map(|dir| markup!("`{}`", format!("{dir}/{file}")))
            .collect::<Vec<_>>()
            .join(" or ");
        Self {
            file: file.to_owned(),
            asked: asked.to_owned(),
            help: format!("{asked} asks for it; write it at {places}"),
        }
    }
}

/// A page whose markup replaced the document root, so typst generated no
/// `<head>` and everything that appends to one silently vanished.
///
/// typst-html owns `<html>`, `<head>` and `<body>`. If a page emits a single
/// `<html>` element, typst hands back the author's root instead of wrapping it,
/// and the generated head goes with it: the charset, the title, and every meta,
/// canonical and verification tag this build appends. Each appender looks the
/// head up and finds nothing, so all three did nothing at all and the build
/// reported success.
///
/// Fatal, and per page rather than per appender: what ships is a document with
/// no encoding declared and no title, which no amount of later passes can
/// repair.
#[derive(Debug, Error, Diagnostic)]
#[error("{} emits the document root, so this build has no `<head>` to write into", Code(.page))]
#[diagnostic(
    code(baudelaire::template::owns_root),
    help(
        "typst-html writes `<html>`, `<head>` and `<body>` itself; a template \
         that emits `<html>` replaces all three, and the charset, the title and \
         every meta tag go with them. Emit the page's contents without a root \
         element and let typst wrap them"
    )
)]
pub struct TemplateOwnsRoot {
    /// The page, as its source path is spelled.
    pub page: String,
}

impl TemplateOwnsRoot {
    /// The refusal for `page`, whose template emitted the root.
    pub fn new(page: impl std::fmt::Display) -> Self {
        Self {
            page: page.to_string(),
        }
    }
}
