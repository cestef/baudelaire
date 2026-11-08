# Architecture notes

Read before changing the area named in a section. Each is a trap that compiles and passes the obvious test.

## Typst and markdown

**Pinned to typst 0.15, with a private-API dependency.** `world/rules/image.rs`
reimplements typst's private `ToCss`, and `world/mod.rs` installs a rule via
`library.rules.replace(Target::Html, IMAGE_RULE)`. A typst bump can silently change
rendered CSS or break the hook; there is no upstream contract for either.

**`#md` is the one native Typst item, and the only reason `typst-library` and
`typst-utils` are direct dependencies.** Everything else baudelaire gives Typst is
*generated source*: a virtual module's `bindings()` emits `#let` lines through
`Let` and its `body()` is hand-written `.typ`. That cannot express `md`, because
lowering markdown is Rust that has to run mid-evaluation, so it is a `#[func]`
bound into `library.global` and wrapped by `@baudelaire/markdown`. The macro
expands to absolute `::typst_library` / `::typst_utils` paths, which is the whole
reason those two crates are named in `Cargo.toml`; nothing imports them. Two
non-obvious consequences: `Library::builder` clones the global scope into `std`
at build time, so anything defined afterwards must re-bind `library.std` or the
module's `dictionary(std)` lookup misses it; and the wrapper forwards arguments
with `..` because that is what carries a caller's spans into Rust, so a mistake
is underlined in the page rather than inside a package nobody has opened.

**Templates must not emit `<html>`, `<head>`, or `<body>`.** typst-html owns those. If a
page emits a single `<html>` element, typst hands back the author's root with no
generated `<head>`, and everything that appends to head (meta tags, verification links)
silently vanishes.

**Single-element typst arrays need the trailing comma**: `("rust",)`. Without it the
frontmatter value is a string, not a list. KDL frontmatter has the same hole and no
comma to close it (one argument is always the scalar), which is one reason `.md` pages
take YAML and TOML too.

**A markdown page's frontmatter dialect is its fence**, and only its fence: `---` is
YAML, `+++` is TOML, `;;;` is KDL. There is no name after the fence and no config key,
so a block is never read as a language it is not. The table is `FENCES` in
`content/markdown/frontmatter/mod.rs`, and a row carries the reader beside the
fence, so adding a dialect is one row plus one module and nothing dispatches on
the dialect a second time. Every dialect resolves its own document into a
`Spans` map keyed by a path of steps *as it parses*, so diagnostics walk one
structure rather than three and no parser's document model has to outlive the parse
(`saphyr` borrows its input, `toml_edit` owns it). Note `toml_edit::ImDocument`, never
`DocumentMut`: the mutable form despans, and every `span()` silently becomes `None`.
`saphyr`'s `Marker::index()` is a *char* index despite its doc comment, so it must go
through a char-to-byte table or any accented character mis-underlines. A repeated key is
gone before the loaded document can be asked about it (YAML keeps the last value, and the
mapping comes back with one entry), so `yaml.rs` replays the parser's own events through
`saphyr-parser` to find one; that second crate is a direct dependency for exactly this.

**`typst { features }` accepts `-name` to disable.** `html` is force-enabled and cannot
be removed. This exists because config lists replace wholesale, so a bare `features`
list would otherwise drop the html default.

## Config, CLI and modules

**Config lists replace wholesale; nested struct sections fill in place.** A profile
overriding one field of `html { }` inherits the siblings; a profile declaring
`collections` replaces the whole set. The policy is written once, on `Section::fill` in
`config/dispatch.rs`, not at each section.

**`src/config/` is one module per config block, nested as the blocks are.** A section's
struct, its `Default`, and its `Section` key table live in the same file
(`config/html/highlight.rs` is `html { highlight { } }`), so adding a key is one edit in
one file. It was three files split by language construct, and that is what made the
config the one extension point that did not honour the one-impl-plus-one-line promise.
`config/mod.rs` holds `Config` itself and re-exports every type flatly, so the rest of
the crate still says `crate::config::HtmlConfig`. Its tests are `config/tests/`, grouped
by theme rather than by section: they drive `Config::parse` over whole config texts, so
they are parser integration tests and splitting them per section would be forced.

**`src/cli/` is one module per subcommand**, each holding its clap args struct, its
`Run` impl and its own helpers. `cli/mod.rs` keeps only what every command shares: `Cli`,
`GlobalArgs`, the overrides, the `Command` enum, `Cx`, `Run`, and `dispatch`. `announce`
and `theme` are gated as whole modules (`#[cfg(feature = ..)]` on the `mod` line), which
replaced a `cfg` on each of their items. Three commands are directories because they are
more than an args struct: `cli/serve/` (the dev server), `cli/scaffold/` (what `init` and
`new` write, beside the `scaffold/` template tree it embeds), `cli/theme.rs` stayed one
file because its verbs share `Says`, `Vendored` and `Ships`.

**A type split across a directory keeps its members `pub(super)`.** `cli/serve/`,
`cli/scaffold/`, `content/frontmatter/` and `content/markdown/lower/` were each one
module, so their fields and methods were private and stayed that way: widening them to
`pub(crate)` would publish internals that only their own directory has ever touched.

**Every `git` process the build starts is started in `src/git/`.** `Repo` is the
entry point, discovered once per `Project` and re-read only when the world is
(a new commit moves the build context, which is what `Project::current` guards).
`run.rs` is the only place a process is spawned, under one set of `-c` options
and with a set of `GIT_*` variables cleared: a repository's own config can make
git run a program (`core.fsmonitor`, `log.showSignature` calling gpg) or change
what it prints, and a stray `GIT_DIR` would answer about another repository
altogether. Every argument is `&'static`, so nothing a site controls can reach
git as an option; the one exception is the `--format` string, which is built
from typed `Field`s. `Unanswered` keeps "could not ask" apart from "answered
nothing", which is the bug the old code had: a failing `git status` read as a
clean tree. The log walk streams, and a record is told from the paths under it
by a leading control character, which git guarantees a path can never start with
because it C-quotes any path holding one.

**A frontmatter schema says two different things, and they live apart.**
`FieldType` is the *shape* and composes (`list<one-of<a|b>>`); `min`, `max`,
`default` and `pattern` are keys on the field line and constrain the *value*.
A `Pattern` is compiled where it is written and carried, since a schema is
checked once per page per field; its identity is the source it was written as,
because a compiled `Regex` has none. `regex` is a direct dependency for it, and
deliberately that one: it is linear-time by construction, so a pattern a site
declares cannot be made to backtrack. A constraint the
type cannot answer for is refused at config-parse time
(`FieldSchema::constrained`), never left to a check that would silently never
fire. `Bound` is the one place that says what a `min` counts for each type and
the words a diagnostic reports it in. A default is applied twice, and must be:
`Frontmatter::defaults` fills the Rust struct that listings and feeds read,
`Prepare::defaults` lays a dict under the page's own in the wrapper so
`page.frontmatter` carries it too. The wrapper is the page's cache fingerprint,
so a changed default recompiles the pages that take it.

## Build pipeline

**A build is `Build` plus `Phase::ORDER`** (`engine/build.rs`). `Engine::run`
is now a prologue and a loop: it sets up only what a phase cannot own without
borrowing from its own state (the plan, the asset pipeline, the pass), hands it
over as `Prologue`, and walks the table. A phase is one variant, one row in
`ORDER`, and one arm of `Phase::run`, so adding one without placing it does not
compile. Two views stay methods rather than fields, and must: an `Output` and an
`&Artifact` borrow the render results beside them, and one struct cannot hold
both. Each phase is timed into a `debug` line, which is where a `--phase` or a
serve that skips one would hang off.

## Extending

Adding a theme source (`theme/source.rs`: where `theme add` can fetch from), an
emitter (`engine/emit/mod.rs`), a render transform
(`render/transform/mod.rs`), an asset handler (`engine/asset/handler.rs`), a sidecar
(`engine/compile/sidecar.rs`: a paged second compile of a page, written beside its
HTML, as `cards` and `pdf` are), or a virtual module (`engine/asset/module.rs` for JS,
`world/module.rs` for Typst) is one `impl` plus one line in that file's `builtin()`.

**A render transform states where it runs, and an emitter states what it reads.**
`Transform::after` names the passes that must precede one, and
`Transforms::builtin` topologically sorts by it, so the hand-written list is a
preference among passes nothing separates rather than the contract; a test holds
the list to being a valid linearization already. `Processor::inputs` names the
slices of the site a whole-site pass reads. Both are required rather than
defaulted, for the same reason `claims` is.

Adding a config block is the same shape: a module under `src/config/` beside its
siblings (or under its parent's directory, if it nests inside one), holding the struct,
its `Default` and its `#[derive(Table)]`; then a `#[key(nested(T))]` field on the parent
and a re-export line in `config/mod.rs`. `config/tests/provenance.rs` fails if a key
parses and reads nothing.

**A section's table is derived, not written.** `#[derive(Table)]` (the `macros/`
crate, `dispatch-derive`) builds the `Section` or `Attributed` table from
`#[key(<shape>)]` fields and their doc comments; the row's doc is the field's
*first doc paragraph*, so a field may carry as much rustdoc below a blank line
as it likes. `<shape>` is a word in `config/vocab.rs`, which is the one place a
`Kind` is tied to its `NodeExt` reader and its `Value` reader: `rule!` for a
`{ }` block, `attr!` for a `key=value` line. `opt <shape>` is the `Option<T>`
form. A key no shape describes stays hand-written, inline on the field, as
`#[key(custom(kind, read, write))]`; a switch is `#[table(hook(switch = field))]`
and anything else associated goes in `#[table(items { .. })]` verbatim. **Field
order is table order**, which is also reference order, so a struct's fields have
to be written in the order the config reads them.

The derive itself knows nothing about KDL or this crate; its tests derive tables
for an unrelated toy vocabulary, which is what keeps it that way. One table stays
hand-written on purpose: `SourcesConfig` is a newtype whose three keys all append
into one `Vec`, so it has no field-per-key mapping to derive. Every conversion is
checked by `reference::the_checked_in_reference_matches_the_dispatch_tables`,
which fails the moment a derived table differs from what shipped.

Adding a config *layer* (an environment overlay, a CLI overlay) is one variant of
`values::source::Layer` plus one push in `Sources::of`; every verb that reports where a
value came from reads it from there.
