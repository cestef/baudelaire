//! A small, ergonomic XML document builder over quick-xml.
//!
//! Wraps quick-xml's event API so the feed and sitemap writers share one
//! escaping-correct surface — leaf, empty, and nested elements — instead of
//! each hand-rolling element serialization.

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use crate::error::Result;

/// An in-progress XML document. Every text and attribute value is escaped by
/// quick-xml, so callers pass raw strings.
pub(super) struct Xml {
    writer: Writer<Vec<u8>>,
}

impl Xml {
    /// Start a document with an `<?xml version="1.0" encoding="UTF-8"?>` decl.
    pub(super) fn document() -> std::io::Result<Self> {
        let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
        Ok(Self { writer })
    }

    /// Write `<name attrs…>` … `</name>`, its body produced by `content`.
    pub(super) fn nest(
        &mut self,
        name: &str,
        attrs: &[(&str, &str)],
        content: impl FnOnce(&mut Xml) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.writer.write_event(Event::Start(Self::start(name, attrs)))?;
        content(self)?;
        self.writer.write_event(Event::End(BytesEnd::new(name.to_owned())))
    }

    /// Write a `<name>text</name>` leaf.
    pub(super) fn leaf(&mut self, name: &str, text: &str) -> std::io::Result<()> {
        self.writer.write_event(Event::Start(Self::start(name, &[])))?;
        self.writer.write_event(Event::Text(BytesText::new(text)))?;
        self.writer.write_event(Event::End(BytesEnd::new(name.to_owned())))
    }

    /// Write a self-closing `<name attrs… />`.
    pub(super) fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) -> std::io::Result<()> {
        self.writer.write_event(Event::Empty(Self::start(name, attrs)))
    }

    /// Finish the document, returning its UTF-8 text.
    pub(super) fn finish(self) -> Result<String> {
        String::from_utf8(self.writer.into_inner()).map_err(|e| std::io::Error::other(e).into())
    }

    fn start(name: &str, attrs: &[(&str, &str)]) -> BytesStart<'static> {
        let mut start = BytesStart::new(name.to_owned());
        for (key, value) in attrs {
            start.push_attribute((*key, *value));
        }
        start
    }
}
