//! TOML frontmatter, between `+++` fences.

use std::ops::Range;

use toml_edit::{Item, TableLike, Value as TomlValue};
use typst::foundations::{Dict, Value};

use super::{Block, Spans};
use crate::error::Result;
use crate::error::markdown::{FrontmatterFault, MarkdownError};
use crate::ui::Text;

/// What a valid block looks like, for the diagnostic on one that is not.
pub const HINT: &str = "a `+++` block is TOML: `title = \"A page\"`";

/// Read a TOML block into its fields and their spans; `ImDocument`, never
/// `DocumentMut`, which despans the document so that every `span()` answers
/// `None`.
pub fn parse(text: &str, offset: usize, path: &str, source: &str) -> Result<Block> {
    let mut reader = Reader::new(text, offset);
    let doc = toml_edit::ImDocument::parse(text).map_err(|error: toml_edit::TomlError| {
        MarkdownError::Frontmatter {
            path: path.to_owned(),
            dialect: "TOML".to_owned(),
            hint: HINT.to_owned(),
            src: miette::NamedSource::new(path, source.to_owned()),
            faults: vec![FrontmatterFault::at(
                Text(error.message()).to_string(),
                reader.shift(error.span().unwrap_or(0..text.len())),
            )],
        }
    })?;
    let dict = reader.fields(doc.as_table(), &[]);
    Ok(Block {
        dict,
        spans: reader.spans,
    })
}

struct Reader {
    /// Where the block starts in the file, folded into every span this records.
    offset: usize,
    spans: Spans,
}

impl Reader {
    /// A reader over `text`, measuring the block's own span rather than asking
    /// for it: a document opening with a `[table]` header reports an empty span
    /// for its root table.
    fn new(text: &str, offset: usize) -> Self {
        let mut reader = Self {
            offset,
            spans: Spans::default(),
        };
        let block = reader.shift(0..text.len());
        reader.spans.insert(Vec::new(), block);
        reader
    }

    /// A span of the block, as a span of the file it sits in.
    fn shift(&self, span: Range<usize>) -> Range<usize> {
        span.start + self.offset..span.end + self.offset
    }

    /// Every entry of a table as a `(key, value)` pair, recording where each
    /// was written on the way down; a dotted key writes no table and so carries
    /// no span, leaving its key segment to underline.
    fn fields(&mut self, table: &dyn TableLike, at: &[String]) -> Dict {
        table
            .iter()
            .map(|(key, item)| {
                let path = Spans::path(at, key);
                let span = item
                    .span()
                    .or_else(|| table.get_key_value(key).and_then(|(k, _)| k.span()));
                if let Some(span) = span {
                    let span = self.shift(span);
                    self.spans.insert(path.clone(), span);
                }
                (key.into(), self.read(item, &path))
            })
            .collect()
    }

    /// What an entry holds, by the shape TOML gave it.
    ///
    /// ```toml
    /// title = "A"        # a value
    /// [author]           # a table
    /// [[post]]           # repeated: an array of tables
    /// ```
    fn read(&mut self, item: &Item, at: &[String]) -> Value {
        match item {
            Item::Value(value) => self.value(value, at),
            Item::Table(table) => Value::Dict(self.fields(table, at)),
            Item::ArrayOfTables(tables) => Value::Array(
                tables
                    .iter()
                    .enumerate()
                    .map(|(i, table)| {
                        let path = Spans::path(at, &i.to_string());
                        if let Some(span) = table.span() {
                            let span = self.shift(span);
                            self.spans.insert(path.clone(), span);
                        }
                        Value::Dict(self.fields(table, &path))
                    })
                    .collect(),
            ),
            Item::None => Value::None,
        }
    }

    /// A TOML value as its typst counterpart, recording where each element of
    /// an array was written on the way through; a datetime becomes its own
    /// spelling, which is the ISO string the frontmatter date reader takes.
    fn value(&mut self, value: &TomlValue, at: &[String]) -> Value {
        match value {
            TomlValue::String(v) => Value::Str(v.value().as_str().into()),
            TomlValue::Integer(v) => Value::Int(*v.value()),
            TomlValue::Float(v) => Value::Float(*v.value()),
            TomlValue::Boolean(v) => Value::Bool(*v.value()),
            TomlValue::Datetime(v) => Value::Str(v.value().to_string().into()),
            TomlValue::Array(array) => Value::Array(
                array
                    .iter()
                    .enumerate()
                    .map(|(i, element)| {
                        let path = Spans::path(at, &i.to_string());
                        if let Some(span) = element.span() {
                            let span = self.shift(span);
                            self.spans.insert(path.clone(), span);
                        }
                        self.value(element, &path)
                    })
                    .collect(),
            ),
            TomlValue::InlineTable(table) => Value::Dict(self.fields(table, at)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(source: &str) -> Dict {
        parse(source, 0, "a.md", source).expect("valid toml").dict
    }

    #[test]
    fn a_value_keeps_the_type_it_was_written_as() {
        let d = dict("title = \"A\"\norder = 3\nratio = 1.5\ndraft = true\n");
        assert_eq!(d.at("title".into(), None), Ok(Value::Str("A".into())));
        assert_eq!(d.at("order".into(), None), Ok(Value::Int(3)));
        assert_eq!(d.at("ratio".into(), None), Ok(Value::Float(1.5)));
        assert_eq!(d.at("draft".into(), None), Ok(Value::Bool(true)));
    }

    #[test]
    fn an_array_is_a_list_of_any_length() {
        let Ok(Value::Array(tags)) = dict("tags = [\"a\", \"b\"]\n").at("tags".into(), None) else {
            panic!("tags should be an array");
        };
        assert_eq!(tags.len(), 2);
        let Ok(Value::Array(one)) = dict("tags = [\"a\"]\n").at("tags".into(), None) else {
            panic!("a one-element array is still an array");
        };
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn a_header_and_an_inline_table_read_the_same() {
        for source in [
            "[author]\nname = \"cstef\"\n",
            "author = { name = \"cstef\" }\n",
        ] {
            let Ok(Value::Dict(author)) = dict(source).at("author".into(), None) else {
                panic!("author should be a dict: {source}");
            };
            assert_eq!(
                author.at("name".into(), None),
                Ok(Value::Str("cstef".into()))
            );
        }
    }

    #[test]
    fn a_repeated_header_is_a_list_of_dicts() {
        let Ok(Value::Array(posts)) =
            dict("[[post]]\nname = \"a\"\n[[post]]\nname = \"b\"\n").at("post".into(), None)
        else {
            panic!("post should be an array");
        };
        assert_eq!(posts.len(), 2);
        let Ok(Value::Dict(first)) = posts.at(0, None) else {
            panic!("each element is a dict");
        };
        assert_eq!(first.at("name".into(), None), Ok(Value::Str("a".into())));
    }

    #[test]
    fn a_date_literal_becomes_the_string_the_date_reader_takes() {
        let d = dict("date = 2026-08-05\nstamp = 2026-08-05T10:00:00Z\n");
        assert_eq!(
            d.at("date".into(), None),
            Ok(Value::Str("2026-08-05".into()))
        );
        assert_eq!(
            d.at("stamp".into(), None),
            Ok(Value::Str("2026-08-05T10:00:00Z".into()))
        );
    }

    #[test]
    fn spans_point_into_the_file_and_reach_nested_keys() {
        let text = "author = { name = \"cstef\" }\ntags = [\"a\", \"b\"]\n";
        let source = format!("+++\n{text}+++\n");
        let block = parse(text, 4, "a.md", &source).expect("valid");
        let at = |path: &[&str]| {
            let steps: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
            block.spans.of(&steps).map(|s| source[s].to_owned())
        };
        assert_eq!(at(&["author", "name"]).as_deref(), Some("\"cstef\""));
        assert_eq!(at(&["author"]).as_deref(), Some("{ name = \"cstef\" }"));
        assert_eq!(at(&["tags", "1"]).as_deref(), Some("\"b\""));
        assert_eq!(
            at(&["author", "email"]).as_deref(),
            Some("{ name = \"cstef\" }")
        );
        assert_eq!(at(&["nothing"]).as_deref(), Some(text));
    }

    #[test]
    fn a_dotted_key_falls_back_to_its_own_segment() {
        let source = "author.name = \"cstef\"\n";
        let block = parse(source, 0, "a.md", source).expect("valid");
        let at = |path: &[&str]| {
            let steps: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
            block.spans.of(&steps).map(|s| source[s].to_owned())
        };
        assert_eq!(at(&["author"]).as_deref(), Some("author"));
        assert_eq!(at(&["author", "name"]).as_deref(), Some("\"cstef\""));
    }

    #[test]
    fn a_quoted_key_holding_a_dot_does_not_collide_with_a_nested_one() {
        let source = "\"a.b\" = 1\n[a]\nb = 2\n";
        let block = parse(source, 0, "a.md", source).expect("valid");
        let at = |path: &[&str]| {
            let steps: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
            block.spans.of(&steps).map(|s| source[s].to_owned())
        };
        assert_eq!(at(&["a.b"]).as_deref(), Some("1"));
        assert_eq!(at(&["a", "b"]).as_deref(), Some("2"));
    }

    #[test]
    fn a_block_that_is_not_toml_is_an_error() {
        let source = "title \"unclosed\n";
        assert!(parse(source, 0, "a.md", source).is_err());
    }
}
