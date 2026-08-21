# dispatch-derive

Derive a dispatch table from a struct's fields, its doc comments, and one macro
you own.

The crate holds no vocabulary. It decides **which** fields become rows, in what
order, under what key and with what documentation; **what a row is** is entirely
yours. Every row is handed to a `macro_rules!` in your crate:

```rust
#[derive(Table)]
#[table(impl = Section, const RULES: Block<Self> = Block, rule = crate::rule)]
struct Highlight {
    /// What every emitted class starts with. Empty for none.
    #[key(text)]
    prefix: String,
}
```

expands to

```rust
impl Section for Highlight {
    const RULES: Block<Self> = Block(&[
        crate::rule!(@row Self, "prefix", prefix, "What every emitted class starts with. Empty for none.", text),
    ]);
}
```

What `text` means, what a row's tuple looks like, what `Block` is: all defined by
your `rule!`.

## Why

A table-driven config keeps its keys in one place, but each row restates what
the field already says: its name, its type's reader, its type's writer, and a
doc string that duplicates the field's own `///`. The derive removes the
restatement and leaves the table.

## The container attribute

Every setting is optional; the defaults are shown.

| Setting | Default | Meaning |
|---|---|---|
| `impl = <path>` | `Section` | The trait the generated impl is of. |
| `const <NAME>: <Type> = <Ctor>` | `const RULES: Block<Self> = Block` | The associated const holding the table, its type, and the constructor wrapping the row slice. |
| `rule = <path>` | `rule` | The macro every row and hook is expanded by. |
| `hook(<name> = <tokens>)` | none | Expands `rule!(@<name> Self, <tokens>);` in item position. Repeatable. |
| `items { <impl items> }` | none | Associated items copied into the impl verbatim. Repeatable. |

`= <Ctor>` is optional: leave it out and the const is the bare slice.

```rust
#[table(impl = Aliases, const NAMES: &'static [&'static str], rule = alias)]
```

Several `#[table(..)]` attributes accumulate; a repeated setting is an override,
not an error.

## The field attribute

A field becomes a row only if it carries `#[key]`. Everything else is skipped,
which is what lets a struct hold state that is not configurable.

| Written | Effect |
|---|---|
| `#[key]` | Row under the field's own name, with an empty spec. |
| `#[key(<spec>)]` | The spec is forwarded verbatim to `rule!`. |
| `#[key(name = <expr>, <spec>)]` | Row under `<expr>` instead of the field's name. Any expression, so a key whose name is a const keeps naming itself through that const. |

`name = ...` is a rename only when spelled exactly so at the front. A spec that
merely starts with the word `name` is left alone.

The spec is arbitrary tokens, which is the escape hatch: a key your vocabulary
has no word for can carry its reader and writer inline, as long as `rule!` has
an arm that accepts them.

```rust
#[key(custom(
    |c: &Self| c.classes.iter().map(..).collect(),
    |c: &mut Self, n: &KdlNode, t: &str| { c.classes = ..; Ok(()) },
))]
classes: Vec<(Token, String)>,
```

## Documentation

The row's doc string is the **first paragraph** of the field's doc comment,
joined into one line. A field may carry as much rustdoc as it likes below a
blank line without any of it reaching the table:

```rust
/// How many of them there are.
///
/// This paragraph is for a reader of the source, and never reaches the table.
#[key(count)]
size: u32,
```

A field with no doc comment gets `""`.

## The protocol

Two invocation shapes, both of which your macro must match:

```rust
macro_rules! rule {
    (@row $t:ty, $key:expr, $field:ident, $doc:literal, $($spec:tt)*) => { .. };
    (@<hook> $t:ty, $($args:tt)*) => { .. };
}
```

`$t` is `Self`, which resolves inside the generated impl. `@row` must expand to
an **expression** (one element of the table's slice); a hook must expand to
**associated items**.

Write the spec arms one per vocabulary word. That table is then the single place
where "this shape of key" is tied to its reader and its writer:

```rust
macro_rules! rule {
    (@row $t:ty, $key:expr, $field:ident, $doc:literal, text) => {
        (
            $key,
            Kind::Text,
            $doc,
            |c: &$t| c.$field.clone().into(),
            |c: &mut $t, n: &KdlNode, t: &str| {
                c.$field = NodeExt::string(n, t, 0)?;
                Ok(())
            },
        )
    };
}
```

## Limits

- Named-field structs only. Tuple structs, unit structs and enums are a compile
  error.
- The derive writes one impl. A second table over the same struct is a second
  `#[derive(Table)]`-bearing newtype, or a hand-written impl.
- Errors inside a spec are reported by your `rule!`, at the spec's own span. The
  derive cannot check something it does not understand.
