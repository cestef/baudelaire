//! Layout files a build was told to use and could not find, and one it found
//! and could not use.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Code, markup};

/// A template the build was pointed at that is in neither the template
/// directory nor the theme's.
///
/// Reported before the first compile, so it names the config line rather than
/// the generated wrapper that imports it.
#[derive(Debug, Error, Diagnostic)]
#[error("template {} was not found", Code(.file))]
#[diagnostic(code(baudelaire::template::missing), help("{help}"))]
pub struct TemplateMissing {
    /// The filename as it was written, relative to the template directory.
    pub file: String,
    /// What asked for it, a config key or a page, carrying its own markup.
    pub asked: String,
    /// Where it was looked for and what to do about it, built here and so
    /// carrying its own markup rather than escaped as foreign text.
    pub help: String,
}

impl TemplateMissing {
    /// `searched` is the directories looked in, in the order they were looked
    /// in.
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
    pub fn new(page: impl std::fmt::Display) -> Self {
        Self {
            page: page.to_string(),
        }
    }
}
