//! A small, ergonomic XML document builder over quick-xml.
//!
//! Wraps quick-xml's event API so the feed and sitemap writers share one
//! escaping-correct surface — leaf, empty, and nested elements — instead of
//! each hand-rolling element serialization.
//!
//! The builder is infallible by construction: it serializes into an in-memory
//! `Vec<u8>` (writes to which cannot fail) and quick-xml only ever emits
//! UTF-8, so neither writing events nor recovering the final string has a
//! reachable error path. Both invariants are documented on the `expect`s.

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

/// An in-progress XML document. Every text and attribute value is escaped by
/// quick-xml, so callers pass raw strings.
pub(super) struct Xml {
    writer: Writer<Vec<u8>>,
}

impl Xml {
    /// Start a document with an `<?xml version="1.0" encoding="UTF-8"?>` decl.
    pub(super) fn document() -> Self {
        let mut xml = Self {
            writer: Writer::new_with_indent(Vec::new(), b' ', 2),
        };
        xml.write(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)));
        xml
    }

    /// Write `<name attrs…>` … `</name>`, its body produced by `content`.
    pub(super) fn nest(&mut self, name: &str, attrs: &[(&str, &str)], content: impl FnOnce(&mut Xml)) {
        self.write(Event::Start(Self::start(name, attrs)));
        content(self);
        self.write(Event::End(BytesEnd::new(name.to_owned())));
    }

    /// Write a `<name>text</name>` leaf.
    pub(super) fn leaf(&mut self, name: &str, text: &str) {
        self.write(Event::Start(Self::start(name, &[])));
        self.write(Event::Text(BytesText::new(text)));
        self.write(Event::End(BytesEnd::new(name.to_owned())));
    }

    /// Write a self-closing `<name attrs… />`.
    pub(super) fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.write(Event::Empty(Self::start(name, attrs)));
    }

    /// Finish the document, returning its text.
    pub(super) fn finish(self) -> String {
        // Invariant: quick-xml serializes events as UTF-8 (declared as such in
        // the document decl), so the buffer is always valid UTF-8.
        String::from_utf8(self.writer.into_inner()).expect("quick-xml emits UTF-8")
    }

    fn write(&mut self, event: Event<'_>) {
        // Invariant: the sink is a Vec<u8>, whose io::Write impl never fails.
        self.writer
            .write_event(event)
            .expect("writing XML to an in-memory buffer cannot fail");
    }

    fn start(name: &str, attrs: &[(&str, &str)]) -> BytesStart<'static> {
        let mut start = BytesStart::new(name.to_owned());
        for (key, value) in attrs {
            start.push_attribute((*key, *value));
        }
        start
    }
}
