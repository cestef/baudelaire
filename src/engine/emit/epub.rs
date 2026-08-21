//! EPUB 3 bundles: a selection of pages as one reflowable book, each chapter
//! the prose the render pass already captured rather than a second compile.

use std::path::PathBuf;

use std::io::Write as _;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::config::{BundleFormat, Config};
use crate::content::{Page, Selection};
use crate::error::{BundleError, Result};

use super::xml::Xml;
use super::{Emit, Processor, Reads, Site};
use crate::engine::compile::bundle::Bundle;

/// What the format pins: the names and namespaces a reader matches on.
struct Epub3;

impl Epub3 {
    /// The media type, written as the zip's first entry.
    const MIME: &'static str = crate::mime::Mime::EPUB;
    /// The entry that carries it, stored rather than deflated, as the spec
    /// pins both.
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
    fn name(&self) -> &'static str {
        "an EPUB bundle"
    }

    /// Nothing: a bundle's file is named for the selection it binds, which the
    /// page set decides, so it is claimed with the pages rather than here.
    fn claims(&self, _config: &Config) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Never: an EPUB is written per bundle, at a path the page set decides.
    fn inputs(&self, _config: &Config) -> Option<&'static [Reads]> {
        None
    }

    fn enabled(&self, config: &Config) -> bool {
        config
            .artifacts
            .bundles
            .iter()
            .any(|(_, bundle)| bundle.active().contains(&BundleFormat::Epub))
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        for bundle in Bundle::planned(site.config, site.pages) {
            if bundle.format() != BundleFormat::Epub {
                continue;
            }
            let path = bundle.path(site.config);
            if out.claimed(&path) {
                continue;
            }
            let book = Book::new(bundle.selection(), site);
            let bytes = book.write()?;
            out.binary(&path, &bytes)?;
            out.wrote_with(&path, format_args!("{} chapters", book.chapters.len()));
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
    /// Its position in the spine, which is its identity inside the container:
    /// the page's own path may hold characters an OPF id cannot.
    index: usize,
    /// The page's prose, as a full-content feed carries it.
    body: &'a str,
}

impl Chapter<'_> {
    /// The manifest id, which the spine names it by.
    fn id(&self) -> String {
        format!("ch{}", self.index)
    }

    /// The file it is written to.
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

    /// The prose the render pass captured for `page`; `None` where its layout
    /// emitted none, and the page is then left out rather than written as an
    /// empty chapter.
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

    /// What the book is stored under: the absolute URL of the file where the
    /// site has one, since a generated identifier would change on every build
    /// and re-add the book to every library that had it.
    fn identifier(&self) -> String {
        let url = self.selection.url(self.config, BundleFormat::Epub.ext());
        self.config.base().map_or_else(
            || format!("urn:baudelaire:{}", self.selection.id),
            |base| base.join(&url),
        )
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
    /// captured for it, re-spelled as XHTML because a content document is
    /// parsed as XML and typst-html leaves a void element unclosed.
    fn chapter(&self, chapter: &Chapter<'_>) -> String {
        let body = super::xhtml::Xhtml::of(chapter.body);
        self.xhtml(chapter.title(), |xml| {
            xml.leaf("h1", chapter.title());
            xml.raw(&body);
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Book, Chapter, Epub3};
    use crate::config::Config;
    use crate::content::{Data, Frontmatter, Page, PageId, Selection};

    /// A content document is parsed as XML, and typst-html leaves every void
    /// element unclosed: a chapter carrying an image used to make the whole
    /// book unreadable to a conforming reader.
    #[test]
    fn a_chapter_carrying_a_void_element_parses_as_xml() {
        let config = Config::default();
        let selection = Selection {
            id: "guide".to_owned(),
            lang: "en",
            pages: Vec::new(),
            title: "Guide".to_owned(),
        };
        let page = Page {
            id: PageId::new("guide", "a"),
            source: PathBuf::from("content/guide/a.typ"),
            frontmatter: Frontmatter {
                title: Some("First".to_owned()),
                ..Frontmatter::default()
            },
            body: String::new(),
            data: Data::Empty,
            collection: "guide".into(),
            permalink: "/guide/a/".into(),
            output: PathBuf::new(),
            template: None,
            lang: "en".into(),
        };
        let book = Book {
            selection: &selection,
            config: &config,
            chapters: Vec::new(),
        };
        let chapter = Chapter {
            page: &page,
            index: 0,
            body: r#"<p>before<br>after<img src="/x.png" alt="a > b"></p>"#,
        };
        let document = book.chapter(&chapter);
        let options = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..roxmltree::ParsingOptions::default()
        };
        roxmltree::Document::parse_with_options(&document, options)
            .unwrap_or_else(|e| panic!("{e}\n{document}"));
        assert!(document.contains("<br />"), "{document}");
        assert!(document.contains(Epub3::NS_XHTML), "{document}");
    }
}
