//! The derive against a vocabulary it has never heard of: a toy settings table
//! with its own trait, its own row type and its own `rule!`.
//!
//! Nothing here resembles the crate that uses the derive in anger, which is the
//! point: every name below is supplied by this file.

#![allow(dead_code)]

use dispatch_derive::Table;

/// One row, in whatever shape the consumer wants: here a key, a reader, a
/// writer, and the documentation the derive lifted off the field.
type Row<T> = (
    &'static str,
    fn(&T) -> String,
    fn(&mut T, &str),
    &'static str,
);

struct Rows<T: 'static>(&'static [Row<T>]);

impl<T> Rows<T> {
    fn set(&self, value: &mut T, key: &str, written: &str) -> bool {
        match self.0.iter().find(|(k, ..)| *k == key) {
            Some((_, _, write, _)) => {
                write(value, written);
                true
            }
            None => false,
        }
    }

    fn get(&self, value: &T, key: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(k, ..)| *k == key)
            .map(|(_, read, _, _)| read(value))
    }

    fn docs(&self) -> Vec<(&'static str, &'static str)> {
        self.0.iter().map(|&(key, _, _, doc)| (key, doc)).collect()
    }
}

trait Settings: Sized + 'static {
    const ROWS: Rows<Self>;
    const LABEL: &'static str = "settings";

    fn present() -> bool {
        false
    }
}

/// The consumer's whole vocabulary: what `word`, `count` and `flag` mean, and
/// what a hook named `marker` expands to.
macro_rules! rule {
    (@row $t:ty, $key:literal, $field:ident, $doc:literal, word) => {
        (
            $key,
            |v: &$t| v.$field.clone(),
            |v: &mut $t, written: &str| written.clone_into(&mut v.$field),
            $doc,
        )
    };
    (@row $t:ty, $key:literal, $field:ident, $doc:literal, count) => {
        (
            $key,
            |v: &$t| v.$field.to_string(),
            |v: &mut $t, written: &str| v.$field = written.parse().unwrap_or_default(),
            $doc,
        )
    };
    (@row $t:ty, $key:literal, $field:ident, $doc:literal, flag) => {
        (
            $key,
            |v: &$t| v.$field.to_string(),
            |v: &mut $t, written: &str| v.$field = written == "on",
            $doc,
        )
    };
    (@row $t:ty, $key:literal, $field:ident, $doc:literal, custom($read:expr, $write:expr $(,)?)) => {
        ($key, $read, $write, $doc)
    };
    (@marker $t:ty, $field:ident) => {
        fn present() -> bool {
            true
        }
    };
}

#[derive(Default, Table)]
#[table(impl = Settings, const ROWS: Rows<Self> = Rows, rule = rule)]
struct Plain {
    /// What the thing is called.
    #[key(word)]
    name: String,

    /// How many of them there are.
    ///
    /// A second paragraph, which the table must not carry.
    #[key(count)]
    size: u32,

    /// Whether it is on.
    #[key(flag)]
    enabled: bool,

    /// Not a key at all, and so absent from the table.
    internal: String,
}

#[test]
fn every_key_field_becomes_a_row_in_declaration_order() {
    let keys: Vec<&str> = Plain::ROWS.docs().iter().map(|&(key, _)| key).collect();
    assert_eq!(keys, ["name", "size", "enabled"]);
}

#[test]
fn a_field_without_the_attribute_is_not_a_key() {
    assert!(Plain::ROWS.get(&Plain::default(), "internal").is_none());
}

#[test]
fn the_row_reads_and_writes_the_field_it_was_derived_from() {
    let mut plain = Plain::default();
    assert!(Plain::ROWS.set(&mut plain, "name", "widget"));
    assert!(Plain::ROWS.set(&mut plain, "size", "7"));
    assert!(Plain::ROWS.set(&mut plain, "enabled", "on"));
    assert_eq!(plain.name, "widget");
    assert_eq!(plain.size, 7);
    assert!(plain.enabled);
    assert_eq!(Plain::ROWS.get(&plain, "size").as_deref(), Some("7"));
}

#[test]
fn an_unknown_key_matches_nothing() {
    assert!(!Plain::ROWS.set(&mut Plain::default(), "nope", "x"));
}

#[test]
fn the_doc_comment_is_the_rows_documentation() {
    assert_eq!(
        Plain::ROWS.docs(),
        [
            ("name", "What the thing is called."),
            ("size", "How many of them there are."),
            ("enabled", "Whether it is on."),
        ]
    );
}

#[derive(Default, Table)]
#[table(
    impl = Settings,
    const ROWS: Rows<Self> = Rows,
    rule = rule,
    hook(marker = flagged),
    items {
        const LABEL: &'static str = "renamed";
    },
)]
struct Fancy {
    /// Written under a name its field does not have.
    #[key(name = "the-name", word)]
    field: String,

    /// A row the vocabulary builds from expressions written here.
    #[key(custom(
        |v: &Self| format!("<{}>", v.wrapped),
        |v: &mut Self, written: &str| written.clone_into(&mut v.wrapped),
    ))]
    wrapped: String,

    flagged: bool,
}

#[test]
fn a_key_may_be_written_under_a_name_of_its_own() {
    let mut fancy = Fancy::default();
    assert!(Fancy::ROWS.set(&mut fancy, "the-name", "x"));
    assert_eq!(fancy.field, "x");
    assert!(Fancy::ROWS.get(&fancy, "field").is_none());
}

#[test]
fn a_row_may_be_written_out_where_the_vocabulary_has_no_word_for_it() {
    let mut fancy = Fancy::default();
    Fancy::ROWS.set(&mut fancy, "wrapped", "y");
    assert_eq!(Fancy::ROWS.get(&fancy, "wrapped").as_deref(), Some("<y>"));
}

#[test]
fn verbatim_items_reach_the_impl() {
    assert_eq!(Fancy::LABEL, "renamed");
    assert_eq!(Plain::LABEL, "settings");
}

#[test]
fn a_hook_is_expanded_by_the_same_vocabulary() {
    assert!(Fancy::present());
    assert!(!Plain::present());
}

/// A second table over the same struct is what `const` and `impl` exist for.
trait Aliases: Sized + 'static {
    const NAMES: &'static [&'static str];
}

macro_rules! alias {
    (@row $t:ty, $key:literal, $field:ident, $doc:literal,) => {
        $key
    };
}

#[derive(Table)]
#[table(impl = Aliases, const NAMES: &'static [&'static str], rule = alias)]
struct Bare {
    #[key]
    first: (),
    #[key]
    second: (),
}

#[test]
fn a_table_needs_neither_a_constructor_nor_a_spec() {
    assert_eq!(Bare::NAMES, ["first", "second"]);
}
