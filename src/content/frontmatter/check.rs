//! Checking a frontmatter value against a declared schema.

use super::origin::At;
use crate::config::{FieldSchema, FieldType};
use crate::error::Result;
use typst::foundations::{Datetime, Dict, Value};
/// Typed accessors over an evaluated frontmatter [`Value`]; the [`At`] each
/// takes lets a type mismatch underline the value. [`ValueExt::str`] is the
/// infallible exception, for `extra` reads, where a non-string is "absent".
pub(super) trait ValueExt {
    fn str(&self) -> Option<String>;
    fn string(&self, at: At<'_>) -> Result<String>;
    fn boolean(&self, at: At<'_>) -> Result<bool>;
    fn integer(&self, at: At<'_>) -> Result<i64>;
    fn date(&self, at: At<'_>) -> Result<time::Date>;
    fn strings(&self, at: At<'_>) -> Result<Vec<String>>;
    /// This value's typst type name with the article that reads before it
    /// (`a string`, `an integer`), for error messages.
    fn kind(&self) -> String;
}

/// One step from the frontmatter dict down to the value a diagnostic is about:
/// a key of a dictionary, or the position of a list element.
#[derive(Debug, Clone)]
pub(crate) enum Step {
    Key(String),
    Index(usize),
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Key(key) => f.write_str(key),
            Self::Index(i) => write!(f, "{i}"),
        }
    }
}

/// The first way a frontmatter value failed the type declared for it, and
/// where; the check stops here.
#[derive(Debug)]
pub(crate) enum Fault {
    Missing {
        path: Vec<Step>,
        want: FieldType,
    },
    Mismatch {
        path: Vec<Step>,
        want: FieldType,
        /// What was there, with its article: see [`Fields::kind`].
        got: String,
    },
}

impl Fault {
    pub(crate) fn path(&self) -> &[Step] {
        let (Self::Missing { path, .. } | Self::Mismatch { path, .. }) = self;
        path
    }

    /// The steps to whatever holds it: where a missing field would go.
    pub(crate) fn parent(&self) -> &[Step] {
        self.path().split_last().map_or(&[], |(_, rest)| rest)
    }

    /// How a diagnostic names the field: dotted, as `authors.1.email`.
    pub(crate) fn key(&self) -> String {
        self.path()
            .iter()
            .map(Step::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// A schema check in progress: the path walked so far, so a fault deep inside a
/// dictionary names the field it happened at rather than the top-level one it
/// happened under.
#[derive(Default)]
pub(crate) struct Check {
    pub(crate) path: Vec<Step>,
}

impl Check {
    /// Every field a schema declares, against the dictionary that should carry
    /// them. Keys the schema does not name pass through unchecked.
    pub(crate) fn dict(&mut self, schema: &[(String, FieldSchema)], dict: &Dict) -> Option<Fault> {
        for (key, field) in schema {
            self.path.push(Step::Key(key.clone()));
            let fault = match dict.get(key.as_str()) {
                Err(_) if field.optional => None,
                Err(_) => Some(Fault::Missing {
                    path: self.path.clone(),
                    want: field.ty.clone(),
                }),
                Ok(value) => self.value(&field.ty, value),
            };
            self.path.pop();
            if fault.is_some() {
                return fault;
            }
        }
        None
    }

    /// One value against one type. Compound types recurse, so the fault names
    /// the element or the nested key that broke rather than the outermost value
    /// containing it.
    pub(super) fn value(&mut self, ty: &FieldType, value: &Value) -> Option<Fault> {
        let fits = match (ty, value) {
            (FieldType::List(inner), Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    self.path.push(Step::Index(i));
                    let fault = self.value(inner, item);
                    self.path.pop();
                    if fault.is_some() {
                        return fault;
                    }
                }
                true
            }
            (FieldType::Dict(fields), Value::Dict(nested)) => return self.dict(fields, nested),
            _ => Self::scalar(ty, value),
        };
        (!fits).then(|| Fault::Mismatch {
            path: self.path.clone(),
            want: ty.clone(),
            got: value.kind(),
        })
    }

    /// Whether a value is the leaf a type asks for. A compound type reaching
    /// here was already handed a value of the wrong shape.
    pub(super) fn scalar(ty: &FieldType, value: &Value) -> bool {
        match ty {
            FieldType::Any => true,
            FieldType::Str => matches!(value, Value::Str(_)),
            FieldType::Bool => matches!(value, Value::Bool(_)),
            FieldType::Int => matches!(value, Value::Int(_)),
            FieldType::Float => matches!(value, Value::Float(_)),
            FieldType::Date => matches!(
                value,
                Value::Datetime(Datetime::Date(_) | Datetime::Datetime(_))
            ),
            FieldType::List(_) | FieldType::Dict(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::config::{FieldSchema, FieldType};
    use typst::foundations::{Dict, Str, Value};

    fn required(ty: FieldType) -> FieldSchema {
        FieldSchema {
            ty,
            optional: false,
        }
    }
    fn list(items: Vec<Value>) -> Value {
        Value::Array(items.into_iter().collect())
    }
    fn text(s: &str) -> Value {
        Value::Str(Str::from(s))
    }
    fn dict(items: Vec<(&str, Value)>) -> Value {
        Value::Dict(
            items
                .into_iter()
                .map(|(k, v)| (Str::from(k), v))
                .collect::<Dict>(),
        )
    }
    fn fits(ty: &FieldType, value: &Value) -> bool {
        Check::default().value(ty, value).is_none()
    }
    #[test]
    fn a_list_fits_only_when_every_element_has_the_declared_type() {
        let strings = FieldType::parse("list").expect("a valid type");
        assert!(fits(&strings, &list(vec![text("a"), text("b")])));
        assert!(fits(&strings, &list(vec![])));
        assert!(!fits(&strings, &list(vec![text("a"), Value::Int(2)])));
        assert!(!fits(&strings, &text("a")));

        let ints = FieldType::parse("list<int>").expect("a valid type");
        assert!(fits(&ints, &list(vec![Value::Int(1), Value::Int(2)])));
        assert!(!fits(&ints, &list(vec![Value::Int(1), text("a")])));

        let nested = FieldType::parse("list<list<int>>").expect("a valid type");
        assert!(fits(&nested, &list(vec![list(vec![Value::Int(1)])])));
        assert!(!fits(&nested, &list(vec![Value::Int(1)])));

        assert!(fits(&FieldType::Any, &Value::Int(2)));
        assert!(!fits(&FieldType::Str, &Value::Int(2)));
    }
    #[test]
    fn a_dict_fits_when_the_fields_it_declares_do() {
        let mut ty = FieldType::parse("list<dict>").expect("a valid type");
        *ty.fields_mut().expect("a dict leaf") = vec![
            ("name".to_owned(), required(FieldType::Str)),
            (
                "age".to_owned(),
                FieldSchema {
                    ty: FieldType::Int,
                    optional: true,
                },
            ),
        ];

        assert!(fits(&ty, &list(vec![dict(vec![("name", text("A"))])])));
        assert!(fits(
            &ty,
            &list(vec![dict(vec![("name", text("A")), ("bio", text("B"))])])
        ));
        assert!(!fits(&ty, &list(vec![dict(vec![("age", Value::Int(3))])])));
        assert!(!fits(
            &ty,
            &list(vec![dict(vec![("name", text("A")), ("age", text("3"))])])
        ));
        assert!(!fits(&ty, &list(vec![text("A")])));
    }
    #[test]
    fn a_fault_names_the_nested_field_it_happened_at() {
        let mut ty = FieldType::parse("list<dict>").expect("a valid type");
        *ty.fields_mut().expect("a dict leaf") =
            vec![("email".to_owned(), required(FieldType::Str))];
        let schema = vec![("authors".to_owned(), required(ty))];
        let value = list(vec![
            dict(vec![("email", text("a@example.com"))]),
            dict(vec![]),
        ]);
        let Value::Dict(frontmatter) = dict(vec![("authors", value)]) else {
            unreachable!("built as a dict")
        };

        let fault = Check::default()
            .dict(&schema, &frontmatter)
            .expect("the second author declares no email");
        assert_eq!(fault.key(), "authors.1.email");
    }
}
