//! YAML frontmatter: what a bare `---` fence means.
//!
//! The default, because it is what every other generator puts between fences,
//! so a post pasted out of one already parses here. It is also the only dialect
//! of the three that resolves a scalar's type as it reads it, which is why
//! nothing below infers one: `2026-08-05` arrives as a string, which is what the
//! date reader takes, and `3` as an integer.

use std::ops::Range;

use saphyr::{AnnotatedMapping, LoadableYamlNode as _, MarkedYaml, Marker, Scalar, YamlData};
use typst::foundations::{Dict, Value};

use super::{Block, Spans};
use crate::error::Result;
use crate::error::markdown::{FrontmatterFault, MarkdownError};
use crate::ui::Text;

/// What a valid block looks like, for the diagnostic on one that is not.
pub const HINT: &str = "a `---` block is YAML, one `key: value` per line: `title: A page`";

/// Read a YAML block into its fields and their spans.
pub fn parse(text: &str, offset: usize, path: &str, source: &str) -> Result<Block> {
    let mut reader = Reader::new(text, offset, source);
    let fault = |message: String, span: Range<usize>| MarkdownError::Frontmatter {
        path: path.to_owned(),
        dialect: "YAML".to_owned(),
        hint: HINT.to_owned(),
        src: miette::NamedSource::new(path, source.to_owned()),
        // One fault, always: saphyr stops at the first thing it cannot read.
        faults: vec![FrontmatterFault::at(message, span)],
    };
    let documents = MarkedYaml::load_from_str(text).map_err(|error| {
        // saphyr's own wording, escaped: it is foreign text, so a `*` or a
        // backtick in it is a character and not markup this crate opened.
        fault(Text(error.info()).to_string(), reader.point(error.marker()))
    })?;

    let dict = match documents.first() {
        // Nothing at all: an empty block, or one that is only comments. Not an
        // error - the empty dict still goes through the field walk, which is
        // what reports a collection's required field as missing.
        //
        // A second document cannot be reached from here anyway: the fence
        // reader ends the block at the `---` line that would have started one.
        None => Dict::new(),
        Some(root) => {
            // Valid YAML, but not fields: a bare `title` line, or a list.
            // Reported rather than read as nothing, because the alternative is
            // every field the page meant to declare going missing at once, with
            // nothing saying why.
            let YamlData::Mapping(mapping) = &root.data else {
                let span = reader.span(root);
                return Err(fault("frontmatter is not a block of fields".to_owned(), span).into());
            };
            reader.fields(mapping, &[])
        }
    };
    Ok(Block {
        dict,
        spans: reader.spans,
    })
}

/// One block being read: what the walk down it needs, and what it collects.
struct Reader<'a> {
    /// Where a marker in this block lands in the file.
    bytes: Bytes,
    /// The file, which every span recorded here indexes into. Held because a
    /// key YAML did not resolve to a string is named by what the author wrote.
    source: &'a str,
    spans: Spans,
}

impl<'a> Reader<'a> {
    /// A reader over `text`, which sits at `offset` in `source`.
    fn new(text: &str, offset: usize, source: &'a str) -> Self {
        let mut reader = Self {
            bytes: Bytes::new(text, offset),
            source,
            spans: Spans::default(),
        };
        // The block itself, so a field the page never wrote underlines the
        // block rather than nothing.
        let block = reader.trim(offset..offset + text.len());
        reader.spans.insert(Vec::new(), block);
        reader
    }

    /// Every entry of a mapping as a `(key, value)` pair, recording where each
    /// was written on the way down.
    ///
    /// An entry is recorded from its key to the end of its value, so a fault in
    /// `title` underlines `title: A page` and not one half of it.
    fn fields(&mut self, mapping: &AnnotatedMapping<'_, MarkedYaml<'_>>, at: &[String]) -> Dict {
        mapping
            .iter()
            .map(|(key, value)| {
                let name = self.name(key);
                let path = Spans::path(at, &name);
                let entry = self.trim(self.span(key).start..self.span(value).end);
                self.spans.insert(path.clone(), entry);
                (name.as_str().into(), self.read(value, &path))
            })
            .collect()
    }

    /// What a node holds, as its typst counterpart.
    fn read(&mut self, node: &MarkedYaml<'_>, at: &[String]) -> Value {
        match &node.data {
            YamlData::Value(scalar) => Self::scalar(scalar),
            YamlData::Sequence(items) => Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        // An element is indexed, so a fault in one underlines
                        // that element and not the whole list.
                        let path = Spans::path(at, &i.to_string());
                        let span = self.span(item);
                        self.spans.insert(path.clone(), span);
                        self.read(item, &path)
                    })
                    .collect(),
            ),
            YamlData::Mapping(mapping) => Value::Dict(self.fields(mapping, at)),
            // A tag says what a node is, which saphyr has already applied to the
            // node underneath. Nothing here distinguishes `!!str 42` from `"42"`.
            YamlData::Tagged(_, inner) => self.read(inner, at),
            // Three the loader never hands back: a representation is the
            // unresolved scalar lazy parsing would leave, an alias is resolved
            // to its anchor's value before this sees it, and `BadValue` marks a
            // node whose contents were taken. Mapped rather than a `panic`, so a
            // saphyr change costs one field its value and not a build.
            YamlData::Representation(text, ..) => Value::Str(text.as_ref().into()),
            YamlData::Alias(_) | YamlData::BadValue => Value::None,
        }
    }

    /// What a mapping entry is keyed by.
    ///
    /// A typst dict is keyed by strings, and YAML resolves a key like any other
    /// node, so `1:` and `true:` arrive as an integer and a boolean. Naming one
    /// by what the author wrote keeps the entry: dropping it would report the
    /// field as missing rather than as the one that is spelled oddly, and the
    /// name then matches the span underlined beside it.
    ///
    /// A key YAML *did* resolve to a string is taken as resolved. Its span
    /// covers the quotes it may have been written with, and `"title"` is not a
    /// field anyone has.
    fn name(&self, key: &MarkedYaml<'_>) -> String {
        match &key.data {
            YamlData::Value(Scalar::String(text)) => text.as_ref().to_owned(),
            _ => self
                .source
                .get(self.span(key))
                .unwrap_or_default()
                .to_owned(),
        }
    }

    /// A YAML scalar as its typst counterpart.
    ///
    /// Total over the variants rather than read through accessors, because
    /// saphyr resolves every scalar as it parses: there is no representation
    /// left to re-interpret, and a new variant should fail to compile here.
    fn scalar(scalar: &Scalar<'_>) -> Value {
        match scalar {
            Scalar::Null => Value::None,
            Scalar::Boolean(flag) => Value::Bool(*flag),
            Scalar::Integer(int) => Value::Int(*int),
            Scalar::FloatingPoint(float) => Value::Float(float.into_inner()),
            Scalar::String(text) => Value::Str(text.as_ref().into()),
        }
    }

    /// Where a node sits in the file.
    fn span(&self, node: &MarkedYaml<'_>) -> Range<usize> {
        let start = self.bytes.at(node.span.start.index());
        let mut end = self.bytes.at(node.span.end.index());
        // A flow collection ends *at* its closing delimiter rather than past
        // it, so `[rust, typst]` would underline `[rust, typst` and a nested
        // one would drop a `}` off the end of every ancestor. Block style ends
        // past its last character, hence the check on the delimiter itself
        // rather than on the style.
        let flow = matches!(node.data, YamlData::Sequence(_) | YamlData::Mapping(_))
            && matches!(self.source[end..].chars().next(), Some(']' | '}'));
        if flow {
            end = self.bytes.at(node.span.end.index() + 1);
        }
        self.trim(start..end)
    }

    /// One character at a marker, as the span to underline. Empty at the end of
    /// the block, where a scan error often lands and there is no character left
    /// to point at.
    fn point(&self, marker: &Marker) -> Range<usize> {
        self.bytes.at(marker.index())..self.bytes.at(marker.index() + 1)
    }

    /// The same span with trailing whitespace dropped.
    ///
    /// A block collection ends at the line break that closed it, and a label
    /// covering a line break is drawn onto the line after it, under text that
    /// has nothing to do with the fault.
    fn trim(&self, span: Range<usize>) -> Range<usize> {
        let text = self.source.get(span.clone()).unwrap_or_default();
        span.start..span.end - (text.len() - text.trim_end().len())
    }
}

/// A block's char-index-to-file-offset table.
///
/// saphyr reports every position as a *char* index, though its own accessor
/// says bytes. Without this, one accented character earlier in a block shifts
/// every span after it, and one inside a value panics on a slice that is not a
/// char boundary. Built once per block rather than counted per marker, because
/// a block has a marker per key, per value and per list element.
///
/// The block's own offset in the file is folded in, so no caller can convert a
/// position and forget to shift it.
struct Bytes {
    /// The file offset each character of the block starts at.
    offsets: Vec<usize>,
    /// Where the block ends, which is where a marker past its last character
    /// points: the end marker of a value that runs to the end of the block.
    end: usize,
}

impl Bytes {
    fn new(text: &str, offset: usize) -> Self {
        Self {
            offsets: text.char_indices().map(|(i, _)| i + offset).collect(),
            end: offset + text.len(),
        }
    }

    /// The file offset of a character index, clamped to the end of the block.
    fn at(&self, chars: usize) -> usize {
        self.offsets.get(chars).copied().unwrap_or(self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(source: &str) -> Dict {
        parse(source, 0, "a.md", source).expect("valid yaml").dict
    }

    /// No inference here: every type is the one saphyr resolved as it read the
    /// scalar, which is what keeps a date a string for the date reader.
    #[test]
    fn a_scalar_keeps_the_type_yaml_resolved() {
        let d = dict("title: A page\norder: 3\nratio: 1.5\ndraft: true\ndate: 2026-08-05\n");
        assert_eq!(d.at("title".into(), None), Ok(Value::Str("A page".into())));
        assert_eq!(d.at("order".into(), None), Ok(Value::Int(3)));
        assert_eq!(d.at("ratio".into(), None), Ok(Value::Float(1.5)));
        assert_eq!(d.at("draft".into(), None), Ok(Value::Bool(true)));
        assert_eq!(
            d.at("date".into(), None),
            Ok(Value::Str("2026-08-05".into()))
        );
    }

    /// Both styles, and a one-element list, which is the thing KDL cannot spell
    /// and pages come here for.
    #[test]
    fn a_list_is_an_array_in_either_style() {
        for source in ["tags:\n  - rust\n  - typst\n", "tags: [rust, typst]\n"] {
            let Ok(Value::Array(tags)) = dict(source).at("tags".into(), None) else {
                panic!("tags should be an array: {source}");
            };
            assert_eq!(tags.len(), 2);
        }
        let Ok(Value::Array(one)) = dict("tags:\n  - rust\n").at("tags".into(), None) else {
            panic!("a one-element list is still a list");
        };
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn a_nested_mapping_is_a_dict() {
        let Ok(Value::Dict(author)) =
            dict("author:\n  name: cstef\n  role: ed\n").at("author".into(), None)
        else {
            panic!("author should be a dict");
        };
        assert_eq!(
            author.at("name".into(), None),
            Ok(Value::Str("cstef".into()))
        );
        assert_eq!(author.at("role".into(), None), Ok(Value::Str("ed".into())));
    }

    #[test]
    fn null_is_none() {
        assert_eq!(
            dict("summary: null\n").at("summary".into(), None),
            Ok(Value::None)
        );
        // A key with nothing after it is the same thing written shorter.
        assert_eq!(
            dict("summary:\n").at("summary".into(), None),
            Ok(Value::None)
        );
    }

    /// A block with no fields is not an error: the walk still runs, and it is
    /// the walk that reports what a collection required and never got.
    #[test]
    fn a_block_with_nothing_in_it_declares_nothing() {
        assert_eq!(dict("").len(), 0);
        assert_eq!(dict("# just a comment\n").len(), 0);
    }

    /// YAML resolves a key like any other node, so these are not strings. The
    /// entry is kept under what the author wrote, which is the name the field
    /// walk then reports as unknown.
    #[test]
    fn a_key_yaml_did_not_resolve_to_a_string_keeps_its_spelling() {
        let d = dict("1: one\ntrue: yes\n");
        assert_eq!(d.at("1".into(), None), Ok(Value::Str("one".into())));
        assert_eq!(d.at("true".into(), None), Ok(Value::Str("yes".into())));
        // A quoted key is resolved, so it does not arrive wearing its quotes.
        assert_eq!(
            dict("\"title\": A\n").at("title".into(), None),
            Ok(Value::Str("A".into()))
        );
    }

    /// Every value knows where it was written, nested ones included, and the
    /// spans are file offsets: a block does not start at the top of the file.
    #[test]
    fn spans_point_into_the_file_and_reach_nested_keys() {
        let source = "---\nauthor:\n  name: cstef\ntags:\n  - rust\n  - typst\n---\n";
        let text = "author:\n  name: cstef\ntags:\n  - rust\n  - typst\n";
        let block = parse(text, 4, "a.md", source).expect("valid");
        let at = |path: &[&str]| {
            let steps: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
            block.spans.of(&steps).map(|s| source[s].to_owned())
        };
        assert_eq!(at(&["author", "name"]).as_deref(), Some("name: cstef"));
        assert_eq!(at(&["author"]).as_deref(), Some("author:\n  name: cstef"));
        assert_eq!(at(&["tags", "1"]).as_deref(), Some("typst"));
        // A key nobody wrote falls back to the block, which is where it goes.
        assert_eq!(at(&["title"]).as_deref(), Some(text.trim_end()));
    }

    /// A flow collection's end marker points at its closing delimiter rather
    /// than past it, so the delimiter has to be put back or the underline stops
    /// one character short.
    #[test]
    fn a_flow_collection_keeps_its_closing_delimiter() {
        let source = "tags: [rust, typst]\n";
        let block = parse(source, 0, "a.md", source).expect("valid");
        let span = block.spans.of(&["tags".to_owned()]).expect("a span");
        assert_eq!(&source[span], "tags: [rust, typst]");
    }

    /// saphyr counts characters and calls them bytes. Without the table, the
    /// two-byte `é` before `tags` shifts every span after it, and a span
    /// landing mid-character panics on the slice rather than misreporting.
    #[test]
    fn a_multi_byte_character_does_not_shift_the_spans_after_it() {
        let source = "---\ntitle: Café\ntags:\n  - café\n---\n";
        let block = parse("title: Café\ntags:\n  - café\n", 4, "a.md", source).expect("valid");
        let at = |path: &[&str]| {
            let steps: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
            block.spans.of(&steps).map(|s| source[s].to_owned())
        };
        assert_eq!(at(&["title"]).as_deref(), Some("title: Café"));
        assert_eq!(at(&["tags", "0"]).as_deref(), Some("café"));
    }

    /// saphyr reports the first fault and stops, so there is one, and its
    /// position is rebased onto the file like every other span here.
    #[test]
    fn a_block_that_is_not_yaml_is_an_error() {
        let source = "---\ntitle: [unclosed\n---\n";
        let Err(err) = parse("title: [unclosed\n", 4, "a.md", source) else {
            panic!("an unclosed flow sequence is not YAML");
        };
        let rendered = format!("{err:?}");
        assert!(rendered.contains("YAML"), "{rendered}");
    }

    /// Valid YAML that is not a mapping declares no fields at all, which is
    /// worth saying once rather than leaving every field to go missing.
    #[test]
    fn a_block_that_is_not_a_mapping_is_an_error() {
        assert!(parse("just a title\n", 0, "a.md", "just a title\n").is_err());
        assert!(parse("- rust\n- typst\n", 0, "a.md", "- rust\n- typst\n").is_err());
    }
}
