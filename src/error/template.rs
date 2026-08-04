//! Layout files a build was told to use and could not find.

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
