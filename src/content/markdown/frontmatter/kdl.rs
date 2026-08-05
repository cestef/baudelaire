//! KDL frontmatter: the language `config.kdl` is written in.
//!
//! Written between `;;;` fences: the semicolon is KDL's own statement
//! terminator, and it is the one of the three spellings CommonMark gives no
//! meaning to at the start of a line.
//!
//! A site already writing KDL config has one language rather than two, and KDL
//! is the only one of the three whose parser reports every fault in a block at
//! once.

use kdl::{KdlDocument, KdlNode, KdlValue};
use typst::foundations::{Dict, Value};

use super::{Block, Spans};
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
                // Every fault at once, which is kdl's own behaviour and worth
                // keeping: the other two dialects can only ever report the first.
                faults: error
                    .diagnostics
                    .iter()
                    .map(|fault| FrontmatterFault::rebased(fault, offset))
                    .collect(),
            })?;
    let mut reader = Reader::new(&doc, offset);
    let dict = reader.fields(&doc, &[]);
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
    /// A reader over `doc`, which sits at `offset` in the file.
    fn new(doc: &KdlDocument, offset: usize) -> Self {
        let mut reader = Self {
            offset,
            spans: Spans::default(),
        };
        // The block itself, so a field the page never wrote underlines the
        // block rather than nothing.
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
        doc.nodes()
            .iter()
            .map(|node| {
                let key = node.name().value();
                let path = Spans::path(at, key);
                let span = self.shift(node.span());
                self.spans.insert(path.clone(), span);
                (key.into(), self.read(node, &path))
            })
            .collect()
    }

    /// What a node holds, by the shape it was written in. The four spellings are
    /// the ones KDL itself distinguishes, so nothing here is a convention a
    /// reader has to learn separately:
    ///
    /// ```kdl
    /// draft                      // a bare flag is true
    /// title "A"                  // one argument is that value
    /// tags "rust" "typst"        // several are a list
    /// author { name "cstef" }    // a block, or `key=value` entries, is a dict
    /// ```
    fn read(&mut self, node: &KdlNode, at: &[String]) -> Value {
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
            let mut dict: Dict = named.into_iter().collect();
            // A block and `key=value` entries on one node are both fields of it,
            // so they land in one dict rather than the block silently winning.
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
        // A list's elements are indexed, so a fault in one underlines that
        // element.
        if args.len() > 1 {
            for (i, entry) in args.iter().enumerate() {
                let span = self.shift(entry.span());
                self.spans.insert(Spans::path(at, &i.to_string()), span);
            }
        }
        let values: Vec<Value> = args.iter().map(|e| Self::scalar(e.value())).collect();
        match <[Value; 1]>::try_from(values) {
            Ok([only]) => only,
            // Zero arguments is a flag that was written, and writing it is the
            // point: `draft` means the same as `draft #true`.
            Err(values) if values.is_empty() => Value::Bool(true),
            Err(values) => Value::Array(values.into_iter().collect()),
        }
    }

    /// A KDL scalar as its typst counterpart. Read through the accessors rather
    /// than matched on the variants, so a new KDL number representation cannot
    /// turn into a silent `none` here.
    // The one lossy step, and it is the honest option: a KDL integer wider than
    // `i64` has no typst counterpart, and a float at least keeps the magnitude
    // where dropping the value would keep nothing.
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

    /// The distinction a single-element list turns on. In typst this is the
    /// trailing comma; in KDL it cannot be written at all, so one argument is
    /// always the scalar. A page needing a one-element list writes it in YAML
    /// or TOML, both of which can spell one.
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

    /// Every value knows where it was written, nested ones included, and the
    /// spans are file offsets: a block does not start at the top of the file.
    #[test]
    fn spans_point_into_the_file_and_reach_nested_keys() {
        let source = ";;;\nauthor { name \"cstef\" }\n;;;\n";
        let block = parse("author { name \"cstef\" }\n", 4, "a.md", source).expect("valid");
        let at = |path: &[&str]| {
            let steps: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
            block.spans.of(&steps).map(|s| source[s].to_owned())
        };
        // kdl's node span runs to the next node, so it carries the space before
        // the closing brace. Underlining one character of trailing whitespace is
        // not worth trimming for.
        assert_eq!(at(&["author", "name"]).as_deref(), Some("name \"cstef\" "));
        assert_eq!(
            at(&["author"]).as_deref(),
            Some("author { name \"cstef\" }")
        );
    }

    /// kdl reports every fault in a block, and each is kept as its own
    /// diagnostic rather than folded into one message.
    #[test]
    fn a_block_that_is_not_kdl_is_an_error() {
        assert!(parse("title \"unclosed\n", 0, "a.md", "title \"unclosed\n").is_err());
    }
}
