//! `generate { bundles { } }`: many pages bound into one document.

use kdl::KdlNode;

use crate::config::Value;
use crate::config::dispatch::Kind::{Choice, Choices, Flag, Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{Named, SortKey};
use crate::error::Result;

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
#[derive(Debug, Clone, Hash)]
pub struct BundleConfig {
    /// Collections to bind, in the order written; empty with
    /// [`site`](Self::site) unset binds nothing.
    pub collections: Vec<String>,
    /// Bind every page instead, whatever collection it belongs to.
    pub site: bool,
    /// The document's title. Unset, a single-collection bundle takes that
    /// collection's title; any other takes the site's.
    pub title: Option<String>,
    /// How the bound pages are ordered. Unset, each collection's own `sort`.
    pub sort: Option<SortKey>,
    pub reverse: bool,
    /// What to write. Empty means `pdf`.
    pub formats: Vec<BundleFormat>,
    /// The paged template the PDF format is laid out with, handed every page at
    /// once rather than one page's template per page.
    pub template: String,
}

impl BundleConfig {
    /// One `guide { .. }` block: the node name is the bundle's id, the
    /// filename stem every format is written under.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let mut cfg = Self::default();
        Self::line(node, text)?;
        if node.children().is_some() {
            cfg.fill(node, text)?;
        }
        Ok((node.name().value().to_owned(), cfg))
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

impl Section for BundleConfig {
    const RULES: Block<Self> = Block(&[
        (
            "collections",
            Texts,
            "Which collections the bundle binds, one word each.",
            |c| c.collections.clone().into(),
            |c, n, t| {
                c.collections = n.words(t)?;
                Ok(())
            },
        ),
        (
            "site",
            Flag,
            "Bind every page in the site rather than named collections.",
            |c| c.site.into(),
            |c, n, t| {
                c.site = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "title",
            Text,
            "The document's title. Unset, the bound collection's title, or the site's.",
            |c| c.title.clone().into(),
            |c, n, t| {
                c.title = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "sort",
            Choice(SortKey::names),
            "How the bound pages are ordered. Unset, each collection's own sort.",
            |c| c.sort.map(Value::named).into(),
            |c, n, t| {
                c.sort = Some(n.arg(t, 0)?.one::<SortKey>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
        (
            "reverse",
            Flag,
            "Reverse the order the pages are bound in.",
            |c| c.reverse.into(),
            |c, n, t| {
                c.reverse = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "formats",
            Choices(BundleFormat::names),
            "What the bundle is written as. Unset, `pdf`.",
            |c| c.formats.iter().copied().map(Value::named).collect(),
            |c, n, t| {
                c.formats = n.mapped::<BundleFormat>(t)?;
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The paged typst template the PDF is laid out with.",
            |c| c.template.clone().into(),
            |c, n, t| {
                c.template = n.string(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
