//! `artifacts { bundles { } }`: many pages bound into one document.

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::config::{Config, Named, SortKey};
use crate::error::{ConfigError, Result};

/// What a bundle is written as, kept separate from the page selection so one
/// selection can target more than one format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BundleFormat {
    /// A typeset document, laid out by a paged Typst template.
    Pdf,
    /// A reflowable EPUB 3, built from the pages as they were rendered.
    Epub,
}

impl Named for BundleFormat {
    const NAMES: &'static [(&'static str, Self)] = &[("pdf", Self::Pdf), ("epub", Self::Epub)];
}

impl BundleFormat {
    /// The extension the file is written under; the only thing that tells two
    /// formats of one bundle apart on disk.
    pub fn ext(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Epub => "epub",
        }
    }

    /// Whether this binary can write it; a format it lacks is a
    /// [`crate::engine::gate`] row, not a silent omission.
    pub fn compiled(self) -> bool {
        match self {
            Self::Pdf => cfg!(feature = "pdf"),
            Self::Epub => cfg!(feature = "epub"),
        }
    }
}

/// One bundle: which pages it binds, in what order, under what title, and what
/// it is written as.
#[derive(Debug, Clone, Hash, Table)]
pub struct BundleConfig {
    /// Which collections the bundle binds, one word each.
    ///
    /// In the order written; empty with [`site`](Self::site) unset binds
    /// nothing.
    #[key(texts)]
    pub collections: Vec<String>,

    /// Bind every page in the site rather than named collections.
    #[key(flag)]
    pub site: bool,

    /// The document's title. Unset, the bound collection's title, or the site's.
    #[key(opt text)]
    pub title: Option<String>,

    /// How the bound pages are ordered. Unset, each collection's own sort.
    #[key(opt choice(SortKey))]
    pub sort: Option<SortKey>,

    /// Reverse the order the pages are bound in.
    #[key(flag)]
    pub reverse: bool,

    /// What the bundle is written as. Unset, `pdf`.
    #[key(choices(BundleFormat))]
    pub formats: Vec<BundleFormat>,

    /// The paged typst template the PDF is laid out with.
    #[key(text)]
    pub template: String,
}

impl BundleConfig {
    /// One `guide { .. }` block: the node name is the bundle's id, the
    /// filename stem every format is written under.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let id = node.name().value();
        if !Config::segment(id) {
            return Err(ConfigError::not_a_name(text, "bundle", id, NodeExt::span(node)).into());
        }
        let mut cfg = Self::default();
        Self::line(node, text)?;
        if node.children().is_some() {
            cfg.fill(node, text)?;
        }
        Ok((id.to_owned(), cfg))
    }

    /// Whether this bundle binds anything at all.
    pub fn enabled(&self) -> bool {
        !self.collections.is_empty() || self.site
    }

    /// The formats to write, with the default applied: empty means `pdf`.
    pub fn formats(&self) -> Vec<BundleFormat> {
        if self.formats.is_empty() {
            vec![BundleFormat::Pdf]
        } else {
            self.formats.clone()
        }
    }

    /// The formats this binary can actually write; anything dropped here was
    /// already reported by the feature gate.
    pub fn active(&self) -> Vec<BundleFormat> {
        if self.enabled() {
            self.formats()
                .into_iter()
                .filter(|format| format.compiled())
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for BundleConfig {
    fn default() -> Self {
        Self {
            collections: Vec::new(),
            site: false,
            title: None,
            sort: None,
            reverse: false,
            formats: Vec::new(),
            template: "book.typ".into(),
        }
    }
}
