//! KDL frontmatter, between `;;;` fences: the language `config.kdl` is written
//! in.

use kdl::{KdlDocument, KdlNode, KdlValue};
use typst::foundations::{Dict, Value};

use super::{Block, Spans, too_deep};
use crate::error::Result;
use crate::error::markdown::{FrontmatterFault, MarkdownError};

/// What a valid block looks like, for the diagnostic on one that is not.
pub const HINT: &str = "a `;;;` block is KDL, the language `config.kdl` uses: `title \"A page\"`";

/// Read a KDL block into its fields and their spans.
pub fn parse(text: &str, offset: usize, path: &str, source: &str) -> Result<Block> {
    let doc: KdlDocument =
        text.parse()
            .map_err(|error: kdl::KdlError| MarkdownError::Frontmatter {
                path: path.to_owned(),
                dialect: "KDL".to_owned(),
                hint: HINT.to_owned(),
                src: miette::NamedSource::new(path, source.to_owned()),
                faults: error
                    .diagnostics
                    .iter()
                    .map(|fault| FrontmatterFault::rebased(fault, offset))
                    .collect(),
            })?;
    let mut reader = Reader::new(&doc, offset);
    let dict = reader.fields(&doc, &[]);
    if let Some((key, span)) = reader.duplicate {
        return Err(MarkdownError::DuplicateKey {
            path: path.to_owned(),
            key,
            src: miette::NamedSource::new(path, source.to_owned()),
            span: (span.start, span.end - span.start).into(),
        }
        .into());
    }
    if let Some(at) = &reader.deep {
        return Err(too_deep(path, source, reader.spans.of(at)));
    }
    if let Some((key, span)) = reader.ambiguous {
        return Err(MarkdownError::AmbiguousNode {
            path: path.to_owned(),
            key,
            src: miette::NamedSource::new(path, source.to_owned()),
            span: (span.start, span.end - span.start).into(),
        }
        .into());
    }
    Ok(Block {
        dict,
        spans: reader.spans,
    })
}

struct Reader {
    /// Where the block starts in the file, folded into every span this records.
    offset: usize,
    spans: Spans,
    /// The first key written as both a value and a dictionary, and the argument
    /// that would have been dropped.
    ambiguous: Option<(String, std::ops::Range<usize>)>,
    /// The first key declared twice at one level, and where the second one is.
    duplicate: Option<(String, std::ops::Range<usize>)>,
    /// The path of the first value that nested past [`super::DEPTH`].
    deep: Option<Vec<String>>,
}

impl Reader {
    /// A reader over `doc`, recording the block's own span so a field the page
    /// never wrote underlines the block rather than nothing.
    fn new(doc: &KdlDocument, offset: usize) -> Self {
        let mut reader = Self {
            offset,
            spans: Spans::default(),
            ambiguous: None,
            duplicate: None,
            deep: None,
        };
        let block = reader.shift(doc.span());
        reader.spans.insert(Vec::new(), block);
        reader
    }

    /// A span of the block, as a span of the file it sits in.
    fn shift(&self, span: miette::SourceSpan) -> std::ops::Range<usize> {
        let start = span.offset() + self.offset;
        start..start + span.len()
    }

    /// Every node of a document as a `(key, value)` pair, recording where each
    /// was written on the way down.
    fn fields(&mut self, doc: &KdlDocument, at: &[String]) -> Dict {
        let mut seen: Vec<&str> = Vec::new();
        doc.nodes()
            .iter()
            .map(|node| {
                let key = node.name().value();
                if seen.contains(&key) && self.duplicate.is_none() {
                    self.duplicate = Some((key.to_owned(), self.shift(node.name().span())));
                }
                seen.push(key);
                let path = Spans::path(at, key);
                let span = self.shift(node.span());
                self.spans.insert(path.clone(), span);
                (key.into(), self.read(node, &path))
            })
            .collect()
    }

    /// What a node holds, by the shape it was written in.
    ///
    /// ```kdl
    /// draft                      // a bare flag is true
    /// title "A"                  // one argument is that value
    /// tags "rust" "typst"        // several are a list
    /// author { name "cstef" }    // a block, or `key=value` entries, is a dict
    /// ```
    fn read(&mut self, node: &KdlNode, at: &[String]) -> Value {
        if at.len() > super::DEPTH {
            self.deep.get_or_insert_with(|| at.to_vec());
            return Value::None;
        }
        let named: Vec<_> = node
            .entries()
            .iter()
            .filter_map(|e| {
                let key = e.name()?.value();
                let span = self.shift(e.span());
                self.spans.insert(Spans::path(at, key), span);
                Some((key.into(), Self::scalar(e.value())))
            })
            .collect();
        let children = node.children().map(|doc| self.fields(doc, at));

        if !named.is_empty() || children.is_some() {
            if let Some(arg) = node.entries().iter().find(|e| e.name().is_none())
                && self.ambiguous.is_none()
            {
                self.ambiguous = Some((node.name().value().to_owned(), self.shift(arg.span())));
            }
            let mut dict: Dict = named.into_iter().collect();
            for (key, value) in children.unwrap_or_default() {
                dict.insert(key, value);
            }
            return Value::Dict(dict);
        }

        let args: Vec<_> = node
            .entries()
            .iter()
            .filter(|e| e.name().is_none())
            .collect();
        if args.len() > 1 {
            for (i, entry) in args.iter().enumerate() {
                let span = self.shift(entry.span());
                self.spans.insert(Spans::path(at, &i.to_string()), span);
            }
        }
        let values: Vec<Value> = args.iter().map(|e| Self::scalar(e.value())).collect();
        match <[Value; 1]>::try_from(values) {
            Ok([only]) => only,
            Err(values) if values.is_empty() => Value::Bool(true),
            Err(values) => Value::Array(values.into_iter().collect()),
        }
    }

    /// A KDL scalar as its typst counterpart, read through the accessors rather
    /// than matched on the variants, so a new KDL number representation cannot
    /// turn into a silent `none` here.
    // A KDL integer wider than `i64` has no typst counterpart; a float keeps
    // its magnitude where dropping the value would keep nothing.
    #[allow(clippy::cast_precision_loss)]
    fn scalar(value: &KdlValue) -> Value {
        if let Some(text) = value.as_string() {
            return Value::Str(text.into());
        }
        if let Some(int) = value.as_integer() {
            return i64::try_from(int).map_or_else(|_| Value::Float(int as f64), Value::Int);
        }
        if let Some(float) = value.as_float() {
            return Value::Float(float);
        }
        if let Some(flag) = value.as_bool() {
            return Value::Bool(flag);
        }
        Value::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(source: &str) -> Dict {
        parse(source, 0, "a.md", source).expect("valid kdl").dict
    }

    #[test]
    fn one_argument_is_a_scalar() {
        let d = dict("title \"A\"\norder 3\nratio 1.5\n");
        assert_eq!(d.at("title".into(), None), Ok(Value::Str("A".into())));
        assert_eq!(d.at("order".into(), None), Ok(Value::Int(3)));
        assert_eq!(d.at("ratio".into(), None), Ok(Value::Float(1.5)));
    }

    #[test]
    fn several_arguments_are_a_list() {
        let Ok(Value::Array(tags)) = dict("tags \"a\" \"b\"\n").at("tags".into(), None) else {
            panic!("tags should be an array");
        };
        assert_eq!(tags.len(), 2);
    }

    /// KDL cannot spell a one-element list, so one argument is always the
    /// scalar and a page needing one writes it in YAML or TOML.
    #[test]
    fn one_argument_is_never_a_list() {
        assert_eq!(
            dict("tags \"a\"\n").at("tags".into(), None),
            Ok(Value::Str("a".into()))
        );
    }

    #[test]
    fn a_bare_node_is_a_flag() {
        assert_eq!(
            dict("draft\n").at("draft".into(), None),
            Ok(Value::Bool(true))
        );
    }

    #[test]
    fn a_block_and_its_entries_are_one_dict() {
        let Ok(Value::Dict(author)) =
            dict("author role=\"ed\" { name \"cstef\" }\n").at("author".into(), None)
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
            dict("summary #null\n").at("summary".into(), None),
            Ok(Value::None)
        );
    }

    /// kdl's node span runs to the next node, so it carries the space before
    /// the closing brace.
    #[test]
    fn spans_point_into_the_file_and_reach_nested_keys() {
        let source = ";;;\nauthor { name \"cstef\" }\n;;;\n";
        let block = parse("author { name \"cstef\" }\n", 4, "a.md", source).expect("valid");
        let at = |path: &[&str]| {
            let steps: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
            block.spans.of(&steps).map(|s| source[s].to_owned())
        };
        assert_eq!(at(&["author", "name"]).as_deref(), Some("name \"cstef\" "));
        assert_eq!(
            at(&["author"]).as_deref(),
            Some("author { name \"cstef\" }")
        );
    }

    #[test]
    fn a_block_that_is_not_kdl_is_an_error() {
        assert!(parse("title \"unclosed\n", 0, "a.md", "title \"unclosed\n").is_err());
    }

    #[test]
    fn a_node_that_is_both_a_value_and_a_dictionary_is_refused() {
        let text = "author \"cstef\" role=\"editor\"\n";
        assert!(parse(text, 0, "a.md", text).is_err());
        assert!(parse("author \"cstef\"\n", 0, "a.md", "author \"cstef\"\n").is_ok());
        let named = "author role=\"editor\"\n";
        assert!(parse(named, 0, "a.md", named).is_ok());
        let block = "author { name \"cstef\" }\n";
        assert!(parse(block, 0, "a.md", block).is_ok());
    }

    #[test]
    fn a_key_declared_twice_is_refused() {
        let text = "title \"A\"\ntitle \"B\"\n";
        assert!(parse(text, 0, "a.md", text).is_err());
        let nested = "title \"A\"\nauthor { title \"B\" }\n";
        assert!(parse(nested, 0, "a.md", nested).is_ok());
    }
}
