//! `generate { bundles { } }`: many pages bound into one document.

use kdl::KdlNode;

use crate::config::dispatch::Kind::{Choice, Choices, Flag, Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{Named, SortKey};
use crate::error::Result;

/// What one bundle is written as.
///
/// A bundle is a *selection* of pages, and a format is what that selection is
/// serialized to. The two are separate because they answer different questions:
/// which pages, in what order, under what title, is the site's editorial
/// decision, and PDF or EPUB is a delivery one. A manual that wants both should
/// not have to state the first one twice.
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
    /// The extension the file is written under, which is also the whole of how
    /// two formats of one bundle are told apart on disk.
    pub fn ext(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Epub => "epub",
        }
    }

    /// Whether this binary can write it. A format a site asks for and the
    /// binary lacks is a [`crate::engine::gate`] row, not a silent omission.
    pub fn compiled(self) -> bool {
        match self {
            Self::Pdf => cfg!(feature = "pdf"),
            Self::Epub => cfg!(feature = "epub"),
        }
    }
}

/// One bundle: which pages it binds, in what order, under what title, and what
/// it is written as.
///
/// The selection is the whole of it. Everything downstream -- the paged compile,
/// the EPUB writer, the cache entry, the prune -- reads this one answer, so a
/// site that changes what a bundle covers changes it in one place.
#[derive(Debug, Clone, Hash)]
pub struct BundleConfig {
    /// Collections to bind, in the order written. Empty with [`site`](Self::site)
    /// unset binds nothing, which the inert-setting table reports.
    pub collections: Vec<String>,
    /// Bind every page instead, whatever collection it belongs to.
    pub site: bool,
    /// The document's title. Unset, a bundle binding one collection takes that
    /// collection's title and any other takes the site's: a title is a thing the
    /// site has already said, and asking for it again is how the two come to
    /// disagree.
    pub title: Option<String>,
    /// How the bound pages are ordered. Unset, each collection's own `sort`,
    /// which is the order the site already shows those pages in.
    pub sort: Option<SortKey>,
    /// Reverse whatever order was chosen.
    pub reverse: bool,
    /// What to write. Empty means `pdf`, which is what a bundle was before
    /// there was a second format.
    pub formats: Vec<BundleFormat>,
    /// The paged template, read only by the PDF format: it is handed every page
    /// at once, and what it does with a run of documents (a title page, a
    /// contents list, running heads) is not what a single page's template does.
    pub template: String,
}

impl BundleConfig {
    /// One `guide { .. }` block: the node name is the bundle's id, which is
    /// also the filename stem every format is written under.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let mut cfg = Self::default();
        if node.children().is_some() {
            cfg.fill(node, text)?;
        }
        Ok((node.name().value().to_owned(), cfg))
    }

    /// Whether this bundle binds anything at all. A block naming no collection
    /// and not the site asks for nothing, which the inert-setting table reports
    /// rather than letting the build write no file in silence.
    pub fn enabled(&self) -> bool {
        !self.collections.is_empty() || self.site
    }

    /// The formats to write, with the default applied: naming none asks for the
    /// one a bundle has always been.
    pub fn formats(&self) -> Vec<BundleFormat> {
        if self.formats.is_empty() {
            vec![BundleFormat::Pdf]
        } else {
            self.formats.clone()
        }
    }

    /// The formats this binary can actually write. What is dropped here is what
    /// the feature gate has already warned about.
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
            |c, n, t| {
                c.collections = n.words(t)?;
                Ok(())
            },
        ),
        (
            "site",
            Flag,
            "Bind every page in the site rather than named collections.",
            |c, n, t| {
                c.site = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "title",
            Text,
            "The document's title. Unset, the bound collection's title, or the site's.",
            |c, n, t| {
                c.title = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "sort",
            Choice(SortKey::names),
            "How the bound pages are ordered. Unset, each collection's own sort.",
            |c, n, t| {
                c.sort = Some(n.arg(t, 0)?.one::<SortKey>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
        (
            "reverse",
            Flag,
            "Reverse the order the pages are bound in.",
            |c, n, t| {
                c.reverse = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "formats",
            Choices(BundleFormat::names),
            "What the bundle is written as. Unset, `pdf`.",
            |c, n, t| {
                c.formats = n.mapped::<BundleFormat>(t)?;
                Ok(())
            },
        ),
        (
            "template",
            Text,
            "The paged typst template the PDF is laid out with.",
            |c, n, t| {
                c.template = n.string(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
