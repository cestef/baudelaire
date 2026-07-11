use std::collections::BTreeMap;

use typst::foundations::{Datetime, Dict, Value};
use typst::syntax::{
    LinkedNode, Source,
    ast::{AstNode, Expr, Markup},
};

use crate::error::Result;

/// Parsed frontmatter for a single page.
#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub date: Option<time::Date>,
    pub draft: bool,
    pub slug: Option<String>,
    pub template: Option<String>,
    pub order: Option<i64>,
    pub redirect: Vec<String>,
    pub taxonomies: BTreeMap<String, Vec<String>>,
    pub extra: BTreeMap<String, Value>,
}

/// The result of extracting frontmatter from a source file.
pub struct Extract {
    /// The parsed frontmatter.
    pub frontmatter: Frontmatter,
    /// The source with the `#frontmatter(...)` call spliced out, so the
    /// remainder compiles cleanly.
    pub body: String,
    /// The frontmatter argument dict literal (e.g. `(title: "x")`), reused
    /// verbatim to pass page data into a layout template.
    pub data: String,
}

impl Frontmatter {
    /// A string value from `extra` (arbitrary frontmatter), if present and a
    /// string — e.g. `description`, `summary`, `image`, `author`.
    pub fn text(&self, key: &str) -> Option<String> {
        self.extra.get(key).and_then(ValueExt::string)
    }

    /// Extract frontmatter from a source file. Returns `None` when the file has
    /// no leading `#frontmatter(...)` call.
    pub fn extract(source: &Source) -> Result<Option<Extract>> {
        let text = source.text();
        let Some(call) = Call::find(source) else {
            return Ok(None);
        };
        let data = &text[call.args()];
        let frontmatter = Self::from_dict(crate::content::eval::dict(data)?)?;
        Ok(Some(Extract {
            frontmatter,
            body: call.splice(text),
            data: data.to_owned(),
        }))
    }

    fn from_dict(dict: Dict) -> Result<Self> {
        let mut fm = Self::default();
        for (key, val) in dict.iter() {
            match key.as_str() {
                "title" => fm.title = val.string(),
                "date" => fm.date = val.date(),
                "draft" => fm.draft = val.boolean().unwrap_or(false),
                "slug" => fm.slug = val.string(),
                "template" => fm.template = val.string(),
                "order" => fm.order = val.integer(),
                "redirect" => fm.redirect = val.string_list(),
                "tags" | "series" if val.is_array() => {
                    fm.taxonomies.insert(key.to_string(), val.string_list());
                }
                _ => {
                    fm.extra.insert(key.to_string(), val.clone());
                }
            }
        }
        Ok(fm)
    }
}

/// A located `#frontmatter(...)` call within a source file.
struct Call {
    range: std::ops::Range<usize>,
    args: std::ops::Range<usize>,
}

impl Call {
    fn find(source: &Source) -> Option<Self> {
        let root = source.root();
        let markup = root.cast::<Markup>()?;
        let call = markup.exprs().find_map(|expr| match expr {
            Expr::FuncCall(c) if Self::is_frontmatter(&c) => Some(c),
            _ => None,
        })?;
        let linked = LinkedNode::new(root);
        let call_node = linked.find(call.to_untyped().span())?;
        let args_node = linked.find(call.args().to_untyped().span())?;
        let mut range = call_node.range();
        // include the leading `#` that triggered the code expression
        let text = source.text();
        if range.start > 0 && text.as_bytes().get(range.start - 1) == Some(&b'#') {
            range.start -= 1;
        }
        Some(Self {
            range,
            args: args_node.range(),
        })
    }

    fn is_frontmatter(call: &typst::syntax::ast::FuncCall) -> bool {
        matches!(call.callee(), Expr::Ident(ident) if ident.get() == "frontmatter")
    }

    fn args(&self) -> std::ops::Range<usize> {
        self.args.clone()
    }

    fn splice(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..self.range.start]);
        out.push('\n');
        out.push_str(&text[self.range.end..]);
        out
    }
}

trait ValueExt {
    fn string(&self) -> Option<String>;
    fn boolean(&self) -> Option<bool>;
    fn integer(&self) -> Option<i64>;
    fn date(&self) -> Option<time::Date>;
    fn string_list(&self) -> Vec<String>;
    fn is_array(&self) -> bool;
}

impl ValueExt for Value {
    fn string(&self) -> Option<String> {
        if let Value::Str(s) = self {
            Some(s.to_string())
        } else {
            None
        }
    }

    fn boolean(&self) -> Option<bool> {
        if let Value::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    fn integer(&self) -> Option<i64> {
        if let Value::Int(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    fn date(&self) -> Option<time::Date> {
        if let Value::Datetime(d) = self {
            match d {
                Datetime::Date(date) => Some(*date),
                Datetime::Datetime(dt) => Some(dt.date()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn string_list(&self) -> Vec<String> {
        if let Value::Array(arr) = self {
            arr.iter().filter_map(|v| v.string()).collect()
        } else {
            Vec::new()
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }
}
