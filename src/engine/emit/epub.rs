//! EPUB bundles: a selection of pages as one reflowable book.
//!
//! A processor rather than a compile, which is the whole difference between
//! this format and the PDF one. A book is the pages *as they were rendered*:
//! each chapter is the prose region of a page the build already produced, with
//! the chrome gone and its URLs made absolute, which is exactly what a
//! full-content feed carries and is captured by the same pass. Nothing here
//! compiles anything, so a site can ship a book without a typesetter.
//!
//! The container is EPUB 3: a zip whose first entry is an uncompressed
//! `mimetype`, a `META-INF/container.xml` naming the package document, an OPF
//! listing every file and the order they are read in, and a navigation
//! document. Every one of them goes through [`Xml`], like every other piece of
//! markup this build writes.

use std::io::Write as _;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::config::{BundleFormat, Config};
use crate::content::{Page, Selection};
use crate::error::{BundleError, Result};

use super::xml::Xml;
use super::{Emit, Processor, Site};

/// What the format pins: the names and namespaces a reader matches on.
///
/// A table rather than literals at each use, because these are the strings that
/// have to be exactly right and are never read by anyone reviewing the code
/// around them: a book a reader refuses is a diff of this block.
struct Epub3;

impl Epub3 {
    /// The media type, written as the zip's first entry.
    const MIME: &'static str = "application/epub+zip";
    /// The entry that carries it, stored rather than deflated: the spec pins
    /// both, and it is how a reader identifies the file at all.
    const MIMETYPE: &'static str = "mimetype";
    /// The one path a reader is guaranteed to open.
    const CONTAINER: &'static str = "META-INF/container.xml";
    /// Where the package document and everything it lists live.
    const ROOT: &'static str = "OEBPS";
    /// The package document, relative to [`ROOT`](Self::ROOT).
    const PACKAGE: &'static str = "content.opf";
    /// The navigation document, relative to [`ROOT`](Self::ROOT).
    const NAV: &'static str = "nav.xhtml";
    /// The extension every content document is written under.
    const XHTML_EXT: &'static str = "xhtml";

    const TYPE_XHTML: &'static str = "application/xhtml+xml";
    const TYPE_PACKAGE: &'static str = "application/oebps-package+xml";

    const NS_CONTAINER: &'static str = "urn:oasis:names:tc:opendocument:xmlns:container";
    const NS_PACKAGE: &'static str = "http://www.idpf.org/2007/opf";
    const NS_DC: &'static str = "http://purl.org/dc/elements/1.1/";
    const NS_XHTML: &'static str = "http://www.w3.org/1999/xhtml";
    const NS_OPS: &'static str = "http://www.idpf.org/2007/ops";

    /// The version this writes, which the package document declares.
    const VERSION: &'static str = "3.0";
    /// The id the package's `unique-identifier` points at.
    const ID: &'static str = "id";

    /// A path inside the container, under the root every content file shares.
    fn at(name: &str) -> String {
        format!("{}/{name}", Self::ROOT)
    }
}

/// The [`Processor`] that writes an EPUB per bundle asking for one.
pub(super) struct Epub;

impl Processor for Epub {
    fn enabled(&self, config: &Config) -> bool {
        config
            .generate
            .bundles
            .iter()
            .any(|(_, bundle)| bundle.active().contains(&BundleFormat::Epub))
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        for (id, cfg) in &site.config.generate.bundles {
            if !cfg.active().contains(&BundleFormat::Epub) {
                continue;
            }
            for selection in Selection::planned(id, cfg, site.config, site.pages) {
                let path = site
                    .config
                    .file(&selection.url(site.config, BundleFormat::Epub.ext()));
                if out.claimed(&path) {
                    continue;
                }
                let book = Book::new(&selection, site);
                let bytes = book.write()?;
                out.binary(&path, &bytes)?;
                out.wrote_with(&path, format_args!("{} chapters", book.chapters.len()));
            }
        }
        Ok(())
    }
}

/// One book: the chapters, in order, and what the package document says about
/// them.
struct Book<'a> {
    selection: &'a Selection<'a>,
    config: &'a Config,
    chapters: Vec<Chapter<'a>>,
}

/// One chapter: the page it came from, and the prose to write for it.
struct Chapter<'a> {
    page: &'a Page,
    /// Its position in the spine, which is the whole of its identity inside the
    /// container: the page's own path may hold characters an OPF id cannot.
    index: usize,
    /// The page's prose, as a full-content feed carries it.
    body: &'a str,
}

impl Chapter<'_> {
    /// The manifest id, which the spine names it by.
    fn id(&self) -> String {
        format!("ch{}", self.index)
    }

    /// The file it is written to, derived from the id so the two cannot drift.
    fn file(&self) -> String {
        format!("{}.{}", self.id(), Epub3::XHTML_EXT)
    }

    /// The chapter's heading, which is the page's own title.
    fn title(&self) -> &str {
        self.page.frontmatter.title.as_deref().unwrap_or_default()
    }
}

impl<'a> Book<'a> {
    fn new(selection: &'a Selection<'a>, site: &'a Site<'a>) -> Self {
        let chapters = selection
            .pages
            .iter()
            .filter_map(|page| Self::prose(page, site).map(|body| (*page, body)))
            .enumerate()
            .map(|(i, (page, body))| Chapter {
                page,
                index: i + 1,
                body,
            })
            .collect();
        Self {
            selection,
            config: site.config,
            chapters,
        }
    }

    /// The prose the render pass captured for `page`.
    ///
    /// A page with none is one whose layout emitted no region and no body,
    /// which is nothing to read: it is left out of the book rather than written
    /// as an empty chapter a reader has to page through.
    fn prose(page: &Page, site: &'a Site<'a>) -> Option<&'a str> {
        site.outputs
            .iter()
            .find(|out| std::ptr::eq(out.page, page))
            .and_then(|out| out.syndicated)
            .map(|prose| prose.0.as_str())
            .filter(|prose| !prose.trim().is_empty())
    }

    /// The whole container, as bytes.
    fn write(&self) -> Result<Vec<u8>> {
        // Every failure below is the same failure -- this book's container --
        // so it is named once here rather than at each `?`. `zip` folds its own
        // io errors in, so one closure covers both halves.
        let fail = |e: zip::result::ZipError| BundleError::epub(&self.selection.id, e);
        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file(
            Epub3::MIMETYPE,
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .map_err(fail)?;
        zip.write_all(Epub3::MIME.as_bytes())
            .map_err(|e| fail(e.into()))?;
        for (path, text) in self.entries() {
            zip.start_file(&path, SimpleFileOptions::default())
                .map_err(fail)?;
            zip.write_all(text.as_bytes()).map_err(|e| fail(e.into()))?;
        }
        Ok(zip.finish().map_err(fail)?.into_inner())
    }

    /// Every entry after the mimetype, in the order they are written.
    fn entries(&self) -> Vec<(String, String)> {
        let mut out = vec![
            (Epub3::CONTAINER.to_owned(), Self::container()),
            (Epub3::at(Epub3::PACKAGE), self.package()),
            (Epub3::at(Epub3::NAV), self.nav()),
        ];
        out.extend(
            self.chapters
                .iter()
                .map(|chapter| (Epub3::at(&chapter.file()), self.chapter(chapter))),
        );
        out
    }

    /// The file naming the package document.
    fn container() -> String {
        let mut xml = Xml::document();
        xml.nest(
            "container",
            &[("version", "1.0"), ("xmlns", Epub3::NS_CONTAINER)],
            |xml| {
                xml.nest("rootfiles", &[], |xml| {
                    xml.empty(
                        "rootfile",
                        &[
                            ("full-path", &Epub3::at(Epub3::PACKAGE)),
                            ("media-type", Epub3::TYPE_PACKAGE),
                        ],
                    );
                });
            },
        );
        xml.finish()
    }

    /// The package document: what the book is, what is in it, and in what order
    /// it is read.
    fn package(&self) -> String {
        let mut xml = Xml::document();
        xml.nest(
            "package",
            &[
                ("xmlns", Epub3::NS_PACKAGE),
                ("version", Epub3::VERSION),
                ("unique-identifier", Epub3::ID),
                ("xml:lang", self.selection.lang),
            ],
            |xml| {
                xml.nest("metadata", &[("xmlns:dc", Epub3::NS_DC)], |xml| {
                    xml.leaf("dc:title", &self.selection.title);
                    xml.leaf("dc:language", self.selection.lang);
                    xml.tagged("dc:identifier", &[("id", Epub3::ID)], &self.identifier());
                    if let Some(author) = self.config.author(self.selection.lang) {
                        xml.leaf("dc:creator", author);
                    }
                });
                xml.nest("manifest", &[], |xml| {
                    xml.empty(
                        "item",
                        &[
                            ("id", "nav"),
                            ("href", Epub3::NAV),
                            ("media-type", Epub3::TYPE_XHTML),
                            ("properties", "nav"),
                        ],
                    );
                    for chapter in &self.chapters {
                        xml.empty(
                            "item",
                            &[
                                ("id", &chapter.id()),
                                ("href", &chapter.file()),
                                ("media-type", Epub3::TYPE_XHTML),
                            ],
                        );
                    }
                });
                xml.nest("spine", &[], |xml| {
                    for chapter in &self.chapters {
                        xml.empty("itemref", &[("idref", &chapter.id())]);
                    }
                });
            },
        );
        xml.finish()
    }

    /// What the book is stored under.
    ///
    /// The absolute URL of the file on a site that has one, since that is
    /// already unique and stable. An EPUB without a unique identifier is
    /// invalid, and a generated one would change on every build and re-add the
    /// book to every library that already had it.
    fn identifier(&self) -> String {
        let url = self.selection.url(self.config, BundleFormat::Epub.ext());
        match self.config.base() {
            Some(base) => base.join(&url),
            None => format!("urn:baudelaire:{}", self.selection.id),
        }
    }

    /// The navigation document: the table of contents, which is also a chapter
    /// a reader may open.
    fn nav(&self) -> String {
        self.xhtml(&self.selection.title, |xml| {
            xml.nest(
                "nav",
                &[("xmlns:epub", Epub3::NS_OPS), ("epub:type", "toc")],
                |xml| {
                    xml.leaf("h1", &self.selection.title);
                    xml.nest("ol", &[], |xml| {
                        for chapter in &self.chapters {
                            xml.nest("li", &[], |xml| {
                                xml.tagged("a", &[("href", &chapter.file())], chapter.title());
                            });
                        }
                    });
                },
            );
        })
    }

    /// One chapter document: the page's title, then the prose the render pass
    /// captured for it.
    ///
    /// The prose goes in through [`Xml::raw`], the same way the single-file
    /// export splices a page fragment: it is markup this build serialized, and
    /// escaping it would print the chapter as source code.
    fn chapter(&self, chapter: &Chapter<'_>) -> String {
        self.xhtml(chapter.title(), |xml| {
            xml.leaf("h1", chapter.title());
            xml.raw(chapter.body);
        })
    }

    /// The XHTML shell every document in the book shares.
    fn xhtml(&self, title: &str, body: impl FnOnce(&mut Xml)) -> String {
        let mut xml = Xml::document();
        xml.doctype("html");
        xml.nest(
            "html",
            &[
                ("xmlns", Epub3::NS_XHTML),
                ("xml:lang", self.selection.lang),
                ("lang", self.selection.lang),
            ],
            |xml| {
                xml.nest("head", &[], |xml| {
                    xml.empty("meta", &[("charset", "utf-8")]);
                    xml.leaf("title", title);
                });
                xml.nest("body", &[], body);
            },
        );
        xml.finish()
    }
}
