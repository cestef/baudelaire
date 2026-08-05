//! TOML frontmatter, between `+++` fences.
//!
//! The dialect Zola and Hugo established, so a post pasted out of either already
//! parses. It is also the only one of the three with types the target does not
//! have: a TOML date is a value, not a string, and [`Reader::value`] is where
//! that is reconciled.

use std::ops::Range;

use toml_edit::{Item, TableLike, Value as TomlValue};
use typst::foundations::{Dict, Value};

use super::{Block, Spans};
use crate::error::Result;
use crate::error::markdown::{FrontmatterFault, MarkdownError};
use crate::ui::Text;

/// What a valid block looks like, for the diagnostic on one that is not.
pub const HINT: &str = "a `+++` block is TOML: `title = \"A page\"`";

/// Read a TOML block into its fields and their spans.
pub fn parse(text: &str, offset: usize, path: &str, source: &str) -> Result<Block> {
    let mut reader = Reader::new(text, offset);
    // `ImDocument`, never `DocumentMut`. Building the mutable document despans
    // it, so every `span()` on it answers `None` and every label in this module
    // would silently collapse onto the block. `FromStr for DocumentMut` goes
    // that way too, which is why this is a `parse` call and not `text.parse()`.
    let doc = toml_edit::ImDocument::parse(text).map_err(|error: toml_edit::TomlError| {
        MarkdownError::Frontmatter {
            path: path.to_owned(),
            dialect: "TOML".to_owned(),
            hint: HINT.to_owned(),
            src: miette::NamedSource::new(path, source.to_owned()),
            // One fault: toml_edit stops at the first, unlike kdl. Its message
            // rather than its `Display`, which renders a line, a column and a
            // caret block of its own - all of which the snippet already is.
            // Escaped on the way in: the text is the parser's, and it is full
            // of backticks (``expected `.`, `=` ``) that are TOML syntax rather
            // than markup this crate wrote.
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

/// One block being read: what the walk down it needs, and what it collects.
struct Reader {
    /// Where the block starts in the file, folded into every span this records
    /// so no step of the walk can hand back one measured against the block.
    offset: usize,
    spans: Spans,
}

impl Reader {
    /// A reader over `text`, which sits at `offset` in the file.
    fn new(text: &str, offset: usize) -> Self {
        let mut reader = Self {
            offset,
            spans: Spans::default(),
        };
        // The block itself, measured rather than asked for: a document that
        // opens with a `[table]` header reports an empty span for its root
        // table, and this is the span a field the page never wrote gets
        // underlined at.
        let block = reader.shift(0..text.len());
        reader.spans.insert(Vec::new(), block);
        reader
    }

    /// A span of the block, as a span of the file it sits in.
    fn shift(&self, span: Range<usize>) -> Range<usize> {
        span.start + self.offset..span.end + self.offset
    }

    /// Every entry of a table as a `(key, value)` pair, recording where each was
    /// written on the way down.
    ///
    /// One walk for both spellings of a table: `[header]` and `{ inline }`
    /// differ in syntax and in nothing this reads, which is what [`TableLike`]
    /// is for.
    fn fields(&mut self, table: &dyn TableLike, at: &[String]) -> Dict {
        table
            .iter()
            .map(|(key, item)| {
                let path = Spans::path(at, key);
                // A dotted key (`a.b.c = 1`, or `{ y.z = 2 }`) builds
                // intermediate tables the author never wrote a delimiter for,
                // and those carry no span. The key segment is what is left and
                // is the honest thing to underline. When even that is absent
                // nothing is recorded, and `Spans::of` falls back to the nearest
                // parent that was written.
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
            // A repeated `[[header]]` is a list of dicts, indexed like any other
            // list so a fault in one underlines that table and not all of them.
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
            // TOML has no null, and a parse never yields this: it is the hole an
            // edit leaves behind, and `TableLike::iter` skips it. Named rather
            // than swept up by a wildcard, so a fifth shape fails to compile.
            Item::None => Value::None,
        }
    }

    /// A TOML value as its typst counterpart, recording where each element of an
    /// array was written on the way through.
    fn value(&mut self, value: &TomlValue, at: &[String]) -> Value {
        match value {
            TomlValue::String(v) => Value::Str(v.value().as_str().into()),
            TomlValue::Integer(v) => Value::Int(*v.value()),
            TomlValue::Float(v) => Value::Float(*v.value()),
            TomlValue::Boolean(v) => Value::Bool(*v.value()),
            // The one place TOML says more than what reads it. `date =
            // 2026-08-05` is a real date literal here, while the frontmatter
            // date reader takes an ISO day *string*, so the literal is rendered
            // back to its own spelling and goes through that same reader.
            // Deliberate, and not lossy: TOML's own text for a datetime is what
            // the reader already accepts, and giving dates a second private path
            // is what this avoids.
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

    /// TOML can spell a one-element list, which KDL cannot, so length is read
    /// rather than assumed.
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

    /// The two spellings of a table are one dict here, and the reader
    /// downstream cannot tell which was written.
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

    /// A date literal reaches the same reader an ISO day string does, so
    /// `date = 2026-08-05` and `date = "2026-08-05"` are one thing downstream.
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

    /// Every value knows where it was written, nested ones and list elements
    /// included, and the spans are file offsets: a block does not start at the
    /// top of the file.
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
        // Never written, so the dict that should have held it is underlined.
        assert_eq!(
            at(&["author", "email"]).as_deref(),
            Some("{ name = \"cstef\" }")
        );
        // Nothing at all, so the block is.
        assert_eq!(at(&["nothing"]).as_deref(), Some(text));
    }

    /// A dotted key writes no table, so there is no table span to point at and
    /// the key segment stands in. Without the fallback this would underline the
    /// whole block for a fault in one field of it.
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

    /// A quoted key is one key that happens to hold a dot, not two steps. While
    /// the span map joined its steps with `.`, this and `[a] b` were the same
    /// entry and each underlined the other's value.
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
