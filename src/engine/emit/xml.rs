//! A small, ergonomic markup builder over quick-xml: the one escaping-correct
//! surface the feed, sitemap and redirect-stub writers share.

use std::borrow::Cow;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

/// An in-progress XML document. Every text and attribute value is escaped by
/// quick-xml, so callers pass raw strings.
///
/// Values are also stripped of the characters XML 1.0 forbids outright, which
/// have no character reference and would make the document unparseable;
/// [`Xml::raw`] is the one exception, as it is to the escaping.
pub(super) struct Xml {
    writer: Writer<Vec<u8>>,
}

impl Xml {
    /// Start a document with an `<?xml version="1.0" encoding="UTF-8"?>` decl.
    pub(super) fn document() -> Self {
        let mut xml = Self::fragment();
        xml.write(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)));
        xml
    }

    /// A declaration-less document, for markup (like an HTML redirect stub)
    /// that carries no `<?xml?>` prolog.
    pub(super) fn fragment() -> Self {
        Self {
            writer: Writer::new_with_indent(Vec::new(), b' ', 2),
        }
    }

    /// Write a `<!DOCTYPE root>` declaration.
    pub(super) fn doctype(&mut self, root: &str) {
        let root = Self::legal(root);
        self.write(Event::DocType(BytesText::new(&root)));
    }

    /// Write escaped text at the current position.
    pub(super) fn text(&mut self, text: &str) {
        let text = Self::legal(text);
        self.write(Event::Text(BytesText::new(&text)));
    }

    /// Write already-serialized markup verbatim, escaping nothing.
    ///
    /// The one escape hatch, for markup this build already serialized;
    /// re-escaping it would render the site as source code. Never reach for it
    /// with authored text.
    pub(super) fn raw(&mut self, markup: &str) {
        self.write(Event::Text(BytesText::from_escaped(markup)));
    }

    /// Write `<name attrs..>` .. `</name>`, its body produced by `content`.
    pub(super) fn nest(
        &mut self,
        name: &str,
        attrs: &[(&str, &str)],
        content: impl FnOnce(&mut Self),
    ) {
        self.write(Event::Start(Self::start(name, attrs)));
        content(self);
        self.write(Event::End(BytesEnd::new(name.to_owned())));
    }

    /// Write a `<name>text</name>` leaf.
    pub(super) fn leaf(&mut self, name: &str, text: &str) {
        self.tagged(name, &[], text);
    }

    /// [`Xml::leaf`] with attributes on the opening tag, as in
    /// `<content type="html">`.
    pub(super) fn tagged(&mut self, name: &str, attrs: &[(&str, &str)], text: &str) {
        self.write(Event::Start(Self::start(name, attrs)));
        let text = Self::legal(text);
        self.write(Event::Text(BytesText::new(&text)));
        self.write(Event::End(BytesEnd::new(name.to_owned())));
    }

    /// Write a self-closing `<name attrs.. />`.
    pub(super) fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.write(Event::Empty(Self::start(name, attrs)));
    }

    /// Finish the document, returning its text.
    pub(super) fn finish(self) -> String {
        String::from_utf8(self.writer.into_inner()).expect("quick-xml emits UTF-8")
    }

    fn write(&mut self, event: Event<'_>) {
        self.writer
            .write_event(event)
            .expect("writing XML to an in-memory buffer cannot fail");
    }

    fn start(name: &str, attrs: &[(&str, &str)]) -> BytesStart<'static> {
        let mut start = BytesStart::new(name.to_owned());
        for (key, value) in attrs {
            let value = Self::legal(value);
            start.push_attribute((*key, value.as_ref()));
        }
        start
    }

    /// `text` with every character XML 1.0 refuses to carry removed, borrowed
    /// unchanged in the ordinary case where there are none.
    fn legal(text: &str) -> Cow<'_, str> {
        if text.contains(Self::forbidden) {
            Cow::Owned(text.chars().filter(|c| !Self::forbidden(*c)).collect())
        } else {
            Cow::Borrowed(text)
        }
    }

    /// Whether `c` is outside XML 1.0's `Char` production: the C0 controls
    /// except tab, line feed and carriage return, plus the two noncharacters at
    /// the end of the basic plane.
    fn forbidden(c: char) -> bool {
        matches!(
            c,
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Xml;

    #[test]
    fn text_and_attributes_are_escaped() {
        let mut xml = Xml::fragment();
        xml.nest("item", &[("title", "a & \"b\"")], |x| x.text("<b> & </b>"));
        assert_eq!(
            xml.finish(),
            "<item title=\"a &amp; &quot;b&quot;\">&lt;b&gt; &amp; &lt;/b&gt;</item>"
        );
    }

    #[test]
    fn a_control_character_is_dropped_rather_than_written() {
        let mut xml = Xml::fragment();
        xml.leaf("title", "Draft\u{1} two");
        xml.empty("link", &[("href", "/a\u{c}/")]);
        let out = xml.finish();
        assert!(out.contains("<title>Draft two</title>"), "{out}");
        assert!(out.contains("href=\"/a/\""), "{out}");
        assert!(!out.contains('\u{1}'), "{out}");
    }

    #[test]
    fn the_three_legal_whitespace_controls_survive() {
        let mut xml = Xml::fragment();
        xml.leaf("summary", "a\tb\nc\rd");
        let out = xml.finish();
        assert!(out.contains("a\tb\nc"), "{out}");
        assert!(out.contains('d'), "{out}");
    }

    #[test]
    fn raw_markup_is_left_alone() {
        let mut xml = Xml::fragment();
        xml.nest("body", &[], |x| x.raw("<p>already markup</p>"));
        assert_eq!(xml.finish(), "<body><p>already markup</p></body>");
    }
}
