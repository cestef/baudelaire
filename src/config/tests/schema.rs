//! A collection's frontmatter schema, as the config declares it.

use super::parse;
use crate::config::{Config, FieldType};
use crate::error::BaudelaireErrorKind;
use miette::Diagnostic;
/// The one attribute scope that does take a block: a `dict` field's own fields.
#[test]
fn a_schema_dict_still_declares_its_fields_in_a_block() {
    let cfg =
        parse(r#"content { collections { posts { schema { editor "dict" { name "str" } } } } }"#);
    let (key, field) = &cfg.content.collections[0].1.schema[0];
    assert_eq!(key, "editor");
    let FieldType::Dict(fields) = &field.ty else {
        panic!("the dict's own fields were not read from its block");
    };
    assert_eq!(fields[0].0, "name");
}

#[test]
fn schema_reads_the_type_as_a_positional_or_an_attribute() {
    let cfg = parse(
        r#"
        content {
            collections {
                blog {
                    schema {
                        title
                        tags "list"
                        hero type="str" optional=#true
                    }
                }
            }
        }
    "#,
    );
    let schema = cfg.schema("blog");
    assert_eq!(schema.len(), 3);
    // A bare field constrains presence and nothing else, and is still required.
    assert_eq!(schema[0].0, "title");
    assert_eq!(schema[0].1.ty, FieldType::Any);
    assert!(!schema[0].1.optional);
    assert_eq!(schema[1].1.ty, FieldType::List(Box::new(FieldType::Str)));
    assert_eq!(schema[2].1.ty, FieldType::Str);
    assert!(schema[2].1.optional);
    // A collection with no schema, and one with no config at all, constrain nothing.
    assert!(cfg.schema("notes").is_empty());
}

/// A field's type is an expression, and a block declares the fields of the
/// dictionary it ends in, however many lists wrap that dictionary.
#[test]
fn schema_reads_nested_types_and_the_fields_of_a_dict() {
    let cfg = parse(
        r#"
        content {
            collections {
                blog {
                    schema {
                        widths "list<int>"
                        editor "dict" {
                            name "str"
                            email "str" optional=#true
                        }
                        authors "list<dict>" {
                            name "str"
                        }
                    }
                }
            }
        }
    "#,
    );
    let schema = cfg.schema("blog");
    assert_eq!(
        schema[0].1.ty,
        FieldType::List(Box::new(FieldType::Int)),
        "a list names what it holds"
    );

    let FieldType::Dict(fields) = &schema[1].1.ty else {
        panic!("expected a dict, got {:?}", schema[1].1.ty);
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, "name");
    assert_eq!(fields[0].1.ty, FieldType::Str);
    assert!(!fields[0].1.optional);
    assert!(
        fields[1].1.optional,
        "a nested field is optional on its own"
    );

    // The block attaches to the dictionary inside the list, not to the list.
    let FieldType::List(inner) = &schema[2].1.ty else {
        panic!("expected a list, got {:?}", schema[2].1.ty);
    };
    let FieldType::Dict(fields) = inner.as_ref() else {
        panic!("expected a dict element, got {inner:?}");
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].0, "name");
}

/// The type language's two failures, each at the line that wrote it.
#[test]
fn schema_refuses_a_broken_type_and_a_block_with_nowhere_to_go() {
    let code = |kdl: &str| {
        let err = Config::parse(kdl).expect_err("should refuse");
        let BaudelaireErrorKind::Config(config) = &err else {
            panic!("expected a config diagnostic, got: {err:?}");
        };
        config.code().map(|c| c.to_string()).expect("a code")
    };
    assert_eq!(
        code("content { collections { blog { schema { hero \"list<int\" } } } }"),
        "baudelaire::config::type_expr"
    );
    assert_eq!(
        code("content { collections { blog { schema { hero \"list<nope>\" } } } }"),
        "baudelaire::config::unknown_value"
    );
    // A block declares what a dictionary holds, so a type with none has no use
    // for one: the mistake fails here rather than being silently dropped.
    assert_eq!(
        code("content { collections { blog { schema { hero \"list<int>\" { name \"str\" } } } } }"),
        "baudelaire::config::field_not_dict"
    );
}

/// A built-in key's type is fixed by the frontmatter reader, so a schema
/// declaring another one could never be satisfied: it fails at the config line
/// rather than on every page of the collection.
#[test]
fn schema_refuses_a_type_a_builtin_key_cannot_hold() {
    let err = Config::parse("content { collections { blog { schema { title \"int\" } } } }")
        .expect_err("should refuse");
    let BaudelaireErrorKind::Config(config) = &err else {
        panic!("expected a config diagnostic, got: {err:?}");
    };
    assert_eq!(
        config.code().map(|c| c.to_string()).as_deref(),
        Some("baudelaire::config::field_conflict")
    );
    // Declaring the type it does hold, or none at all, is how you require it.
    assert_eq!(
        parse("content { collections { blog { schema { title \"str\" } } } }").schema("blog")[0]
            .1
            .ty,
        FieldType::Str
    );
    assert_eq!(
        parse("content { collections { blog { schema { date } } } }").schema("blog")[0]
            .1
            .ty,
        FieldType::Any
    );
}
