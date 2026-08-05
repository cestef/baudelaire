//! A markdown page's `---` KDL block, as the dict the rest of the pipeline
//! already reads.
//!
//! Converted rather than interpreted a second time. The dict goes through the
//! same [`FIELDS`](crate::content::frontmatter) walk a typst page's export
//! does, so built-in keys, configured taxonomy keys, the typo suggester and the
//! collection schema are all the ones already there, and a new frontmatter key
//! is still one row in one table.

use kdl::{KdlDocument, KdlNode, KdlValue};
use typst::foundations::{Dict, Value};

/// A KDL block read as frontmatter fields.
pub struct Fields<'a>(pub &'a KdlDocument);

impl From<Fields<'_>> for Dict {
    fn from(fields: Fields<'_>) -> Self {
        fields.0.nodes().iter().map(entry).collect()
    }
}

/// One node as a `(key, value)` pair.
fn entry(node: &KdlNode) -> (typst::foundations::Str, Value) {
    (node.name().value().into(), read(node))
}

/// What a node holds, by the shape it was written in. The four spellings are
/// the ones KDL itself distinguishes, so nothing here is a convention a reader
/// has to learn separately:
///
/// ```kdl
/// draft                      // a bare flag is true
/// title "A"                  // one argument is that value
/// tags "rust" "typst"        // several are a list
/// author { name "cstef" }    // a block, or `key=value` entries, is a dict
/// ```
fn read(node: &KdlNode) -> Value {
    let named: Vec<_> = node
        .entries()
        .iter()
        .filter_map(|e| Some((e.name()?.value().into(), scalar(e.value()))))
        .collect();
    let children = node.children().map(|doc| Dict::from(Fields(doc)));

    if !named.is_empty() || children.is_some() {
        let mut dict: Dict = named.into_iter().collect();
        // A block and `key=value` entries on one node are both fields of it, so
        // they land in one dict rather than the block silently winning.
        for (key, value) in children.unwrap_or_default() {
            dict.insert(key, value);
        }
        return Value::Dict(dict);
    }

    let args: Vec<_> = node
        .entries()
        .iter()
        .filter(|e| e.name().is_none())
        .map(|e| scalar(e.value()))
        .collect();
    match <[Value; 1]>::try_from(args) {
        Ok([only]) => only,
        // Zero arguments is a flag that was written, and writing it is the
        // point: `draft` means the same as `draft #true`.
        Err(args) if args.is_empty() => Value::Bool(true),
        Err(args) => Value::Array(args.into_iter().collect()),
    }
}

/// A KDL scalar as its typst counterpart. Read through the accessors rather
/// than matched on the variants, so a new KDL number representation cannot turn
/// into a silent `none` here.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(source: &str) -> Dict {
        let doc: KdlDocument = source.parse().expect("valid kdl");
        Dict::from(Fields(&doc))
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
    /// always the scalar.
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
}
