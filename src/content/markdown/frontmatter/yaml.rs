//! YAML frontmatter, between `---` fences.

use std::collections::BTreeSet;
use std::ops::Range;

use saphyr::{AnnotatedMapping, LoadableYamlNode as _, MarkedYaml, Marker, Scalar, YamlData};
use saphyr_parser::{Event, Parser, Span, SpannedEventReceiver};
use typst::foundations::{Dict, Value};

use super::{Block, Spans, too_deep};
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
        faults: vec![FrontmatterFault::at(message, span)],
    };
    let documents = MarkedYaml::load_from_str(text)
        .map_err(|error| fault(Text(error.info()).to_string(), reader.point(error.marker())))?;

    if let Some((key, at)) = Repeats::scan(text) {
        return Err(MarkdownError::DuplicateKey {
            path: path.to_owned(),
            key,
            src: miette::NamedSource::new(path, source.to_owned()),
            span: reader.bytes.spanning(at).into(),
        }
        .into());
    }

    let dict = match documents.first() {
        None => Dict::new(),
        Some(root) => {
            let YamlData::Mapping(mapping) = &root.data else {
                let span = reader.span(root);
                return Err(fault("frontmatter is not a block of fields".to_owned(), span).into());
            };
            reader.fields(mapping, &[])
        }
    };
    if let Some(at) = &reader.deep {
        return Err(too_deep(path, source, reader.spans.of(at)));
    }
    Ok(Block {
        dict,
        spans: reader.spans,
    })
}

struct Reader<'a> {
    /// Where a marker in this block lands in the file.
    bytes: Bytes,
    /// The file, which every span recorded here indexes into.
    source: &'a str,
    spans: Spans,
    /// The path of the first value that nested past [`super::DEPTH`].
    deep: Option<Vec<String>>,
}

impl<'a> Reader<'a> {
    /// A reader over `text`, recording the block's own span so a field the page
    /// never wrote underlines the block rather than nothing.
    fn new(text: &str, offset: usize, source: &'a str) -> Self {
        let mut reader = Self {
            bytes: Bytes::new(text, offset),
            source,
            spans: Spans::default(),
            deep: None,
        };
        let block = reader.trim(offset..offset + text.len());
        reader.spans.insert(Vec::new(), block);
        reader
    }

    /// Every entry of a mapping as a `(key, value)` pair, recorded from its key
    /// to the end of its value so a fault in `title` underlines the whole of
    /// `title: A page`.
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
        if at.len() > super::DEPTH {
            self.deep.get_or_insert_with(|| at.to_vec());
            return Value::None;
        }
        match &node.data {
            YamlData::Value(scalar) => Self::scalar(scalar),
            YamlData::Sequence(items) => Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let path = Spans::path(at, &i.to_string());
                        let span = self.span(item);
                        self.spans.insert(path.clone(), span);
                        self.read(item, &path)
                    })
                    .collect(),
            ),
            YamlData::Mapping(mapping) => Value::Dict(self.fields(mapping, at)),
            YamlData::Tagged(_, inner) => self.read(inner, at),
            YamlData::Representation(text, ..) => Value::Str(text.as_ref().into()),
            YamlData::Alias(_) | YamlData::BadValue => Value::None,
        }
    }

    /// What a mapping entry is keyed by: a key YAML resolved to something other
    /// than a string (`1:`, `true:`) is named by what the author wrote, so the
    /// entry is kept and reported under that spelling.
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

    /// A YAML scalar as its typst counterpart, taking the type saphyr resolved
    /// as it parsed.
    fn scalar(scalar: &Scalar<'_>) -> Value {
        match scalar {
            Scalar::Null => Value::None,
            Scalar::Boolean(flag) => Value::Bool(*flag),
            Scalar::Integer(int) => Value::Int(*int),
            Scalar::FloatingPoint(float) => Value::Float(float.into_inner()),
            Scalar::String(text) => Value::Str(text.as_ref().into()),
        }
    }

    /// Where a node sits in the file; a flow collection ends *at* its closing
    /// delimiter rather than past it, so that delimiter is put back.
    fn span(&self, node: &MarkedYaml<'_>) -> Range<usize> {
        let start = self.bytes.at(node.span.start.index());
        let mut end = self.bytes.at(node.span.end.index());
        let flow = matches!(node.data, YamlData::Sequence(_) | YamlData::Mapping(_))
            && matches!(self.source[end..].chars().next(), Some(']' | '}'));
        if flow {
            end = self.bytes.at(node.span.end.index() + 1);
        }
        self.trim(start..end)
    }

    /// One character at a marker, as the span to underline.
    fn point(&self, marker: &Marker) -> Range<usize> {
        self.bytes.point(marker.index())
    }

    /// The same span with trailing whitespace dropped, since a label covering a
    /// line break is drawn onto the line after it.
    fn trim(&self, span: Range<usize>) -> Range<usize> {
        let text = self.source.get(span.clone()).unwrap_or_default();
        span.start..span.end - (text.len() - text.trim_end().len())
    }
}

/// The first key a mapping in the block wrote twice, found by replaying the
/// parser's own events.
///
/// A second pass, because the loaded document cannot answer: YAML keeps the last
/// value for a repeated key, so the mapping saphyr hands back holds one entry
/// where the author wrote two. KDL and TOML both refuse the same mistake, and a
/// page that silently carries a value its author cannot see they wrote is worse
/// than one that fails.
#[derive(Default)]
struct Repeats {
    /// One frame per open collection, innermost last; `None` for a sequence,
    /// which keys nothing.
    open: Vec<Option<Keyed>>,
    found: Option<(String, Range<usize>)>,
}

/// One open mapping: what it has been keyed by, and whether the next node it
/// takes is another key.
struct Keyed {
    seen: BTreeSet<String>,
    key: bool,
}

impl Default for Keyed {
    fn default() -> Self {
        Self {
            seen: BTreeSet::new(),
            key: true,
        }
    }
}

impl Repeats {
    /// The repeated key and the char-index range of its second spelling, or
    /// `None` where every mapping keyed each of its entries once.
    ///
    /// `text` has already loaded, so a parse that fails here is one this cannot
    /// speak for and answers nothing rather than a second diagnostic.
    fn scan(text: &str) -> Option<(String, Range<usize>)> {
        let mut repeats = Self::default();
        Parser::new_from_str(text).load(&mut repeats, true).ok()?;
        repeats.found
    }

    /// Take one node against the innermost open collection: a mapping alternates
    /// key and value, and a repeat is a key it already holds.
    fn took(&mut self, name: Option<&str>, span: Span) {
        let Some(Some(frame)) = self.open.last_mut() else {
            return;
        };
        let repeated = frame.key && name.is_some_and(|name| !frame.seen.insert(name.to_owned()));
        frame.key = !frame.key;
        if repeated && self.found.is_none() {
            let name = name.unwrap_or_default().to_owned();
            self.found = Some((name, span.start.index()..span.end.index()));
        }
    }
}

impl<'input> SpannedEventReceiver<'input> for Repeats {
    fn on_event(&mut self, ev: Event<'input>, span: Span) {
        match ev {
            Event::Scalar(value, ..) => self.took(Some(value.as_ref()), span),
            Event::Alias(_) => self.took(None, span),
            Event::MappingStart(..) => {
                self.took(None, span);
                self.open.push(Some(Keyed::default()));
            }
            Event::SequenceStart(..) => {
                self.took(None, span);
                self.open.push(None);
            }
            Event::MappingEnd | Event::SequenceEnd => {
                self.open.pop();
            }
            _ => {}
        }
    }
}

/// A block's char-index-to-file-offset table, with the block's own offset in
/// the file folded in.
///
/// saphyr reports every position as a *char* index, though its own accessor
/// says bytes, so without this an accented character shifts every span after it
/// and a slice lands off a char boundary.
struct Bytes {
    /// The file offset each character of the block starts at.
    offsets: Vec<usize>,
    /// Where the block ends, which is where a marker past its last character
    /// points.
    end: usize,
    /// The character index of the last thing the author actually wrote, which
    /// is as far as a *label* may reach.
    last: usize,
}

impl Bytes {
    fn new(text: &str, offset: usize) -> Self {
        Self {
            offsets: text.char_indices().map(|(i, _)| i + offset).collect(),
            end: offset + text.len(),
            last: text
                .chars()
                .enumerate()
                .filter_map(|(i, c)| (!c.is_whitespace()).then_some(i))
                .last()
                .unwrap_or(0),
        }
    }

    /// The file offset of a character index, clamped to the end of the block.
    fn at(&self, chars: usize) -> usize {
        self.offsets.get(chars).copied().unwrap_or(self.end)
    }

    /// The one character at a character index, as the span to underline,
    /// clamped to the block's last written character so a scan error's marker
    /// does not land on the closing fence.
    fn point(&self, chars: usize) -> Range<usize> {
        let at = chars.min(self.last);
        self.at(at)..self.at(at + 1)
    }

    /// A range of character indices as the file offsets it covers.
    fn spanning(&self, chars: Range<usize>) -> (usize, usize) {
        let start = self.at(chars.start);
        (start, self.at(chars.end).saturating_sub(start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(source: &str) -> Dict {
        parse(source, 0, "a.md", source).expect("valid yaml").dict
    }

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
        assert_eq!(
            dict("summary:\n").at("summary".into(), None),
            Ok(Value::None)
        );
    }

    #[test]
    fn a_block_with_nothing_in_it_declares_nothing() {
        assert_eq!(dict("").len(), 0);
        assert_eq!(dict("# just a comment\n").len(), 0);
    }

    #[test]
    fn a_key_yaml_did_not_resolve_to_a_string_keeps_its_spelling() {
        let d = dict("1: one\ntrue: yes\n");
        assert_eq!(d.at("1".into(), None), Ok(Value::Str("one".into())));
        assert_eq!(d.at("true".into(), None), Ok(Value::Str("yes".into())));
        assert_eq!(
            dict("\"title\": A\n").at("title".into(), None),
            Ok(Value::Str("A".into()))
        );
    }

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
        assert_eq!(at(&["title"]).as_deref(), Some(text.trim_end()));
    }

    #[test]
    fn a_flow_collection_keeps_its_closing_delimiter() {
        let source = "tags: [rust, typst]\n";
        let block = parse(source, 0, "a.md", source).expect("valid");
        let span = block.spans.of(&["tags".to_owned()]).expect("a span");
        assert_eq!(&source[span], "tags: [rust, typst]");
    }

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

    #[test]
    fn a_block_that_is_not_yaml_is_an_error() {
        let source = "---\ntitle: [unclosed\n---\n";
        let Err(err) = parse("title: [unclosed\n", 4, "a.md", source) else {
            panic!("an unclosed flow sequence is not YAML");
        };
        let rendered = format!("{err:?}");
        assert!(rendered.contains("YAML"), "{rendered}");
    }

    #[test]
    fn a_marker_past_the_block_underlines_its_last_written_character() {
        let source = "---\ntitle: Café\nbad: [unclosed\n---\n";
        let text = "title: Café\nbad: [unclosed\n";
        let bytes = Bytes::new(text, 4);
        let span = bytes.point(text.chars().count() + 3);
        assert_eq!(&source[span.clone()], "d");
        assert!(span.end <= 4 + text.trim_end().len(), "{span:?}");
        let at = text.find("Café").expect("in the block") + "Caf".len();
        assert_eq!(&source[bytes.point(text[..at].chars().count())], "é");
    }

    /// YAML keeps the last value for a repeated key, so a page that wrote one
    /// twice carries a value its author cannot see they wrote.
    #[test]
    fn a_key_written_twice_is_an_error() {
        let source = "---\ntitle: A\ntitle: B\n---\n";
        let Err(err) = parse("title: A\ntitle: B\n", 4, "a.md", source) else {
            panic!("a repeated key is not valid frontmatter");
        };
        let rendered = format!("{err:?}");
        assert!(rendered.contains("title"), "{rendered}");

        let Some(label) = miette::Diagnostic::labels(&err)
            .and_then(|mut labels| labels.next())
            .map(|label| label.offset()..label.offset() + label.len())
        else {
            panic!("the error carries a label");
        };
        assert_eq!(&source[label.clone()], "title", "{label:?}");
        assert!(label.start > source.find("title: B").expect("in the block") - 1);
    }

    #[test]
    fn a_key_repeated_inside_a_nested_mapping_is_an_error() {
        let text = "author:\n  name: a\n  name: b\n";
        assert!(parse(text, 0, "a.md", text).is_err());
    }

    /// One key per mapping, not per block: two mappings in a list may each key
    /// `name`, and a key under one heading does not claim the same word under
    /// another.
    #[test]
    fn the_same_key_in_two_mappings_is_not_a_repeat() {
        for text in [
            "authors:\n  - name: a\n  - name: b\n",
            "author:\n  name: a\nreviewer:\n  name: b\n",
            "tags:\n  - rust\n  - rust\n",
        ] {
            assert!(parse(text, 0, "a.md", text).is_ok(), "{text}");
        }
    }

    #[test]
    fn a_block_that_is_not_a_mapping_is_an_error() {
        assert!(parse("just a title\n", 0, "a.md", "just a title\n").is_err());
        assert!(parse("- rust\n- typst\n", 0, "a.md", "- rust\n- typst\n").is_err());
    }
}
