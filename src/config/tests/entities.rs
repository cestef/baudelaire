//! `content { entities { } }`: registries, their shapes, slots and sources.

use super::parse;
use crate::config::{Config, RegistryConfig, SourceConfig, Unknown};
use crate::error::BaudelaireErrorKind;
use miette::Diagnostic;

/// The one registry a config declares, by id.
fn registry<'a>(config: &'a Config, id: &str) -> &'a RegistryConfig {
    config
        .content
        .entities
        .iter()
        .find(|(name, _)| name == id)
        .map(|(_, registry)| registry)
        .expect("the declared registry")
}

fn error(text: &str) -> BaudelaireErrorKind {
    Config::parse(text).expect_err("this config is refused")
}

fn code(text: &str) -> String {
    error(text)
        .code()
        .map(|code| code.to_string())
        .unwrap_or_default()
}

/// A shape is a field set and its slots, and naming one is the whole of what a
/// registry has to write for the common case.
#[test]
fn a_shape_seeds_the_fields_and_the_slots() {
    let config = parse("content {\n  entities {\n    people { shape \"person\" }\n  }\n}");
    let people = registry(&config, "people");
    let fields: Vec<&str> = people.fields.iter().map(|(key, _)| key.as_str()).collect();
    assert!(fields.contains(&"name"), "person declares a name");
    assert!(fields.contains(&"avatar"), "person declares an avatar");
    assert_eq!(people.slots.display.as_deref(), Some("name"));
    assert_eq!(people.slots.image.as_deref(), Some("avatar"));
}

/// A slot the registry spells itself wins, and its siblings still come from the
/// shape: the fill-in-place policy every config section follows.
#[test]
fn a_declared_slot_overrides_the_shape_and_keeps_its_siblings() {
    let config = parse(
        r#"
        content {
          entities {
            people {
              shape "person"
              slots display="url"
            }
          }
        }
    "#,
    );
    let people = registry(&config, "people");
    assert_eq!(people.slots.display.as_deref(), Some("url"));
    assert_eq!(people.slots.image.as_deref(), Some("avatar"));
}

/// Fields fill in place over the shape's, key by key: a shape *is* a field
/// set, so replacing the set wholesale would make naming both keys a
/// contradiction.
#[test]
fn declared_fields_fill_over_the_shapes_own() {
    let config = parse(
        r#"
        content {
          entities {
            people {
              shape "person"
              fields {
                name "str"
                handle "str" optional=#true
              }
            }
          }
        }
    "#,
    );
    let people = registry(&config, "people");
    let name = people
        .fields
        .iter()
        .find(|(key, _)| key == "name")
        .expect("the shape's own field");
    assert!(
        !name.1.optional,
        "the registry required what the shape typed"
    );
    assert!(
        people.fields.iter().any(|(key, _)| key == "avatar"),
        "a field the registry did not mention is still the shape's"
    );
    assert!(
        people.fields.iter().any(|(key, _)| key == "handle"),
        "a field the shape never had is added"
    );
}

/// Sources are read in the order they are written, and every kind is one of
/// them: what makes "a roster fills what the pages left out" a matter of which
/// line came first.
#[test]
fn sources_keep_the_order_they_were_written_in() {
    let config = parse(
        r#"
        content {
          entities {
            people {
              sources {
                pages "content/people"
                data "data/people.kdl"
                inline {
                  zoe { name "Zoe" }
                }
              }
            }
          }
        }
    "#,
    );
    let names: Vec<&str> = registry(&config, "people")
        .sources
        .iter()
        .map(SourceConfig::name)
        .collect();
    assert_eq!(names, ["pages", "data", "inline"]);
}

/// An inline entity is a free table, so a registry's own fields need no config
/// key of their own and a list is written as a list.
#[test]
fn an_inline_entity_carries_whatever_fields_it_writes() {
    let config = parse(
        r#"
        content {
          entities {
            people {
              sources {
                inline {
                  zoe { name "Zoe"; alias "quill" "zed" }
                }
              }
            }
          }
        }
    "#,
    );
    let SourceConfig::Inline(inline) = &registry(&config, "people").sources[0] else {
        panic!("the declared source is the inline one")
    };
    let zoe = &inline.entities[0];
    assert_eq!(zoe.id, "zoe");
    let keys: Vec<&str> = zoe.fields.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["name", "alias"]);
    // Every field is located, so a fault found while the registry is built can
    // still underline the line that wrote it.
    let located: Vec<&str> = zoe.spans.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(located, keys);
}

/// A registry with a source is a roster, and a name that is not in it is a
/// typo. One with no source knows nobody, so every term is taken as written.
#[test]
fn the_unknown_policy_defaults_to_whether_there_is_a_roster() {
    let bare = parse("content {\n  entities {\n    people { shape \"person\" }\n  }\n}");
    assert_eq!(registry(&bare, "people").unknown(), Unknown::Synthesize);

    let rostered = parse(
        r#"
        content {
          entities {
            people { sources { data "data/people.kdl" } }
          }
        }
    "#,
    );
    assert_eq!(registry(&rostered, "people").unknown(), Unknown::Error);

    let asked = parse(
        r#"
        content {
          entities {
            people {
              unknown "warn"
              sources { data "data/people.kdl" }
            }
          }
        }
    "#,
    );
    assert_eq!(registry(&asked, "people").unknown(), Unknown::Warn);
}

/// A slot pointing at a field nobody declares renders every entity without it,
/// out of a green build. It is refused at the line that wrote it instead.
#[test]
fn a_slot_must_name_a_declared_field() {
    assert_eq!(
        code(
            r#"
            content {
              entities {
                people {
                  fields { name "str" }
                  slots image="protrait"
                }
              }
            }
        "#
        ),
        "baudelaire::config::entity_slot"
    );
    // ..but a registry that declares no fields constrains nothing, so its slots
    // name whatever its sources happen to carry.
    let loose = parse(
        r#"
        content {
          entities {
            people { slots image="portrait" }
          }
        }
    "#,
    );
    assert_eq!(
        registry(&loose, "people").slots.image.as_deref(),
        Some("portrait")
    );
}

/// Two registries of one id would silently lose one of them.
#[test]
fn a_repeated_registry_id_is_refused() {
    assert_eq!(
        code("content {\n  entities {\n    people { }\n    people { }\n  }\n}"),
        "baudelaire::config::duplicate_id"
    );
}
