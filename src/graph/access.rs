//! Fine-grained tracking of which *values* a page reads from a structured input.
//!
//! Some values reach typst not as files but as injected data: `sys.inputs.*`,
//! and build metadata at `sys.inputs.baudelaire`. Reading one is an in-language
//! dictionary access that the file-dependency tracker never sees, so a change to
//! it can't be pinned to the pages it affects. This module recovers the read set
//! statically from the syntax tree, letting a page depend on
//! `sys.inputs.baudelaire.git.hash` alone and rebuild only when *that* value
//! changes, not on every commit.
//!
//! It is generic over the root value. Give [`reads`] a set of [`Root`]s — each a
//! dotted base that names the value in source (`"sys.inputs.baudelaire"`) and
//! its current [`Value`] — and it returns the qualified paths read from each.
//! The same roots drive [`digest`], so analysis and invalidation share one
//! source of truth and cannot drift.
//!
//! The analysis is sound by over-approximation: it never misses a read (which
//! would serve stale output), but where it cannot narrow an access — a dynamic
//! `.at(key)`, the whole base bound to a variable — it widens to the base
//! itself, i.e. "depends on the entire value". Precise for the direct-access and
//! `let`-alias patterns templates actually use.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use typst::syntax::{
    Source, SyntaxNode,
    ast::{self, AstNode, Expr},
};

use super::Hash;
use crate::codegen::Value;
use crate::graph::Deps;
use crate::world::Project;

/// A structured value exposed to typst whose reads we track. `base` is the
/// dotted identifier chain naming it in source; `tree` is its current value.
#[derive(Clone, Copy)]
pub struct Root<'a> {
    pub base: &'a str,
    pub tree: &'a Value,
}

/// The qualified value paths a source reads, e.g. `"sys.inputs.baudelaire.git.hash"`.
/// A bare base means the whole value was read (or couldn't be narrowed).
pub type Reads = BTreeSet<String>;

/// The value paths `source` reads from `roots`.
pub fn reads(source: &Source, roots: &[Root]) -> Reads {
    let mut out = Reads::new();
    visit(source.root(), &mut Env::new(), roots, &mut out);
    out
}

/// The content digest of the value at a qualified `key`. Equal across builds iff
/// the value is unchanged; an absent path hashes to a stable marker, so its
/// later appearance still invalidates.
pub fn digest(roots: &[Root], key: &str) -> Hash {
    match locate(roots, key) {
        Some(value) => Hash::of_bytes(&serde_json::to_vec(value).unwrap_or_default()),
        None => Hash::of_bytes(ABSENT),
    }
}

/// The digest of every key in `keys`, ready to store in a cache entry.
pub fn digests(roots: &[Root], keys: &Reads) -> BTreeMap<String, Hash> {
    keys.iter()
        .map(|key| (key.clone(), digest(roots, key)))
        .collect()
}

/// Sentinel hashed for a key that resolves to no value — distinct from any real
/// serialized value, so a path gaining a value later reads as a change.
const ABSENT: &[u8] = b"\0baudelaire::access::absent";

/// Variable bindings that alias into a tracked value, e.g. the `git` in
/// `#let git = sys.inputs.baudelaire.git`. Maps the name to the access path it
/// stands for, so later `git.hash` resolves to the full path.
type Env = HashMap<String, Vec<String>>;

/// Walk a node, updating `env` at `let` bindings and recording the read of every
/// maximal access expression. Recording happens at the outermost resolvable node
/// and then descends only into call arguments (never the callee chain), so a
/// chain is recorded once, not once per link.
fn visit(node: &SyntaxNode, env: &mut Env, roots: &[Root], out: &mut Reads) {
    if let Some(expr) = node.cast::<Expr>() {
        match expr {
            // A binding is not a read: the bound value only becomes a dependency
            // where it is *used*. Record `let`s in `env` and scan the
            // initializer's arguments (a metadata default is still a read).
            Expr::LetBinding(binding) => {
                bind(binding, env);
                if let Some(init) = binding.init() {
                    visit_init(init.to_untyped(), env, roots, out);
                }
                return;
            }
            _ => {
                if let Some(path) = resolve(&expr, env) {
                    record(&path, roots, out);
                    if let Expr::FuncCall(call) = expr {
                        for arg in call.args().items() {
                            visit(arg.to_untyped(), env, roots, out);
                        }
                    }
                    return;
                }
            }
        }
    }
    for child in node.children() {
        visit(child, env, roots, out);
    }
}

/// Visit a `let` initializer without recording the bound chain itself, but still
/// catching reads nested in its call arguments.
fn visit_init(node: &SyntaxNode, env: &mut Env, roots: &[Root], out: &mut Reads) {
    if let Some(expr) = node.cast::<Expr>()
        && resolve(&expr, env).is_some()
    {
        if let Expr::FuncCall(call) = expr {
            for arg in call.args().items() {
                visit(arg.to_untyped(), env, roots, out);
            }
        }
        return;
    }
    visit(node, env, roots, out);
}

/// Record `#let name = <access>` so later uses of `name` resolve through it.
fn bind(binding: ast::LetBinding, env: &mut Env) {
    if let ast::LetBindingKind::Normal(ast::Pattern::Normal(Expr::Ident(name))) = binding.kind()
        && let Some(path) = binding.init().and_then(|init| resolve(&init, env))
    {
        env.insert(name.get().to_string(), path);
    }
}

/// Resolve an access expression to its path from the global scope, or `None` if
/// it isn't a plain access. Follows identifiers (through `env` aliases), field
/// access, `.at("key")`, and the collection methods that expose a whole value.
fn resolve(expr: &Expr, env: &Env) -> Option<Vec<String>> {
    match expr {
        Expr::Ident(ident) => {
            let name = ident.get();
            Some(
                env.get(name.as_str())
                    .cloned()
                    .unwrap_or_else(|| vec![name.to_string()]),
            )
        }
        Expr::FieldAccess(access) => {
            let mut path = resolve(&access.target(), env)?;
            path.push(access.field().get().to_string());
            Some(path)
        }
        Expr::FuncCall(call) => resolve_call(*call, env),
        Expr::Parenthesized(paren) => resolve(&paren.expr(), env),
        _ => None,
    }
}

/// Resolve `<target>.method(..)` for the accessors that read a value: `.at("k")`
/// narrows to that key; a dynamic `.at(expr)` and the collection accessors
/// (`.keys`, `.values`, ..) widen to the whole target.
fn resolve_call(call: ast::FuncCall, env: &Env) -> Option<Vec<String>> {
    let Expr::FieldAccess(access) = call.callee() else {
        return None;
    };
    let mut path = resolve(&access.target(), env)?;
    match access.field().get().as_str() {
        "at" => {
            if let Some(Expr::Str(key)) = first_positional(call) {
                path.push(key.get().to_string());
            }
            Some(path)
        }
        "keys" | "values" | "pairs" | "len" | "has" | "contains" => Some(path),
        _ => None,
    }
}

/// The first positional argument of a call, if any.
fn first_positional(call: ast::FuncCall) -> Option<Expr> {
    call.args().items().find_map(|arg| match arg {
        ast::Arg::Pos(expr) => Some(expr),
        _ => None,
    })
}

/// Match a resolved access path against each root and record the qualified key
/// it reads.
fn record(path: &[String], roots: &[Root], out: &mut Reads) {
    for root in roots {
        if let Some(key) = qualified(path, root) {
            out.insert(key);
        }
    }
}

/// The qualified key `path` reads from `root`, or `None` if it doesn't touch it.
/// A path that stops short of, equals, or grabs a non-narrowable part of the
/// base yields the base itself (the whole value); a longer path is truncated at
/// the first leaf it reaches (trailing segments are method calls on the value).
fn qualified(path: &[String], root: &Root) -> Option<String> {
    let path: Vec<&str> = path.iter().map(String::as_str).collect();
    let base: Vec<&str> = root.base.split('.').collect();
    let shared = base.len().min(path.len());
    if path[..shared] != base[..shared] {
        return None;
    }
    if path.len() <= base.len() {
        // the access is the base, or a prefix of it (a superset): the whole value.
        return Some(root.base.to_string());
    }
    let mut key = root.base.to_string();
    let mut node = root.tree;
    for segment in &path[base.len()..] {
        let Value::Dict(_) = node else {
            break; // a leaf; the rest are method calls on it.
        };
        key.push('.');
        key.push_str(segment);
        match node.get(segment) {
            Some(child) => node = child,
            None => break, // an absent key; its presence is the dependency.
        }
    }
    Some(key)
}

/// The value at a qualified `key`, or `None` if no root owns it or the path is
/// absent.
fn locate<'a>(roots: &'a [Root], key: &str) -> Option<&'a Value> {
    for root in roots {
        if key == root.base {
            return Some(root.tree);
        }
        if let Some(rest) = key
            .strip_prefix(root.base)
            .and_then(|r| r.strip_prefix('.'))
        {
            let mut node = root.tree;
            for segment in rest.split('.') {
                node = node.get(segment)?;
            }
            return Some(node);
        }
    }
    None
}

/// A build-scoped analyzer: the tracked roots plus a per-file memo, so a template
/// shared by hundreds of pages is analyzed once, not once per page.
pub struct Analyzer<'a> {
    roots: Vec<Root<'a>>,
    project: &'a Project,
    memo: Mutex<HashMap<PathBuf, Arc<Reads>>>,
}

impl<'a> Analyzer<'a> {
    /// Build an analyzer over `roots` for the pages of `project`.
    pub fn new(roots: Vec<Root<'a>>, project: &'a Project) -> Self {
        Self {
            roots,
            project,
            memo: Mutex::new(HashMap::new()),
        }
    }

    /// The value paths a page reads: its own compiled source (which carries any
    /// inline generated body) unioned with every `.typ` file it depends on
    /// (templates, imported modules, its `#include`d body).
    pub fn reads(&self, source: &Source, deps: &Deps) -> Reads {
        let mut out = reads(source, &self.roots);
        for path in deps.files() {
            if path.extension().is_some_and(|ext| ext == "typ") {
                out.extend(self.file(path).iter().cloned());
            }
        }
        out
    }

    /// The reads of a dependency file, hashed once per build.
    fn file(&self, path: &Path) -> Arc<Reads> {
        if let Some(cached) = self.memo.lock().get(path) {
            return cached.clone();
        }
        let found = self
            .project
            .source(path)
            .map(|source| reads(&source, &self.roots))
            .unwrap_or_default();
        let found = Arc::new(found);
        self.memo.lock().insert(path.to_owned(), found.clone());
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Value {
        Value::dict([
            ("version", Value::str("0.1.0")),
            ("date", Value::str("2026-07-16")),
            (
                "git",
                Value::dict([
                    ("hash", Value::str("abc123")),
                    ("dirty", Value::Bool(false)),
                ]),
            ),
        ])
    }

    fn read(code: &str) -> Reads {
        let source = Source::detached(code);
        let tree = tree();
        reads(
            &source,
            &[Root {
                base: "sys.inputs.baudelaire",
                tree: &tree,
            }],
        )
    }

    fn keys(reads: &Reads) -> Vec<&str> {
        reads.iter().map(String::as_str).collect()
    }

    #[test]
    fn direct_leaf_access() {
        assert_eq!(
            keys(&read("#sys.inputs.baudelaire.git.hash")),
            ["sys.inputs.baudelaire.git.hash"]
        );
    }

    #[test]
    fn method_on_a_leaf_is_truncated() {
        // `.slice` is a method on the string, not a deeper value.
        assert_eq!(
            keys(&read("#sys.inputs.baudelaire.git.hash.slice(0, 7)")),
            ["sys.inputs.baudelaire.git.hash"]
        );
    }

    #[test]
    fn literal_at_is_a_field() {
        assert_eq!(
            keys(&read("#sys.inputs.baudelaire.at(\"version\")")),
            ["sys.inputs.baudelaire.version"]
        );
    }

    #[test]
    fn let_aliases_are_followed() {
        // The exact shape a theme uses: bind the root, then a subtree, then read.
        let code = r#"
            #let build = sys.inputs.at("baudelaire", default: (:))
            #let git = build.at("git", default: none)
            #build.version #git.hash
        "#;
        assert_eq!(
            keys(&read(code)),
            [
                "sys.inputs.baudelaire.git.hash",
                "sys.inputs.baudelaire.version"
            ]
        );
    }

    #[test]
    fn binding_alone_is_not_a_read() {
        // Binding the whole context must NOT record a whole-context dependency;
        // only the fields actually read count.
        let code = r#"
            #let build = sys.inputs.baudelaire
            #build.version
        "#;
        assert_eq!(keys(&read(code)), ["sys.inputs.baudelaire.version"]);
    }

    #[test]
    fn subtree_grab_records_the_subtree() {
        assert_eq!(
            keys(&read("#let g = sys.inputs.baudelaire.git\n#g")),
            ["sys.inputs.baudelaire.git"]
        );
    }

    #[test]
    fn whole_context_when_grabbed_directly() {
        assert_eq!(
            keys(&read("#let x = sys.inputs.baudelaire\n#x")),
            ["sys.inputs.baudelaire"]
        );
    }

    #[test]
    fn dynamic_key_widens_to_the_target() {
        let code = "#let k = \"git\"\n#sys.inputs.baudelaire.at(k)";
        assert_eq!(keys(&read(code)), ["sys.inputs.baudelaire"]);
    }

    #[test]
    fn a_superset_of_inputs_widens_to_the_whole_value() {
        // Grabbing all of `sys.inputs` could read baudelaire, so depend on it whole.
        assert_eq!(
            keys(&read("#let all = sys.inputs\n#all")),
            ["sys.inputs.baudelaire"]
        );
    }

    #[test]
    fn unrelated_access_is_ignored() {
        assert!(read("#page.frontmatter.title #sys.version").is_empty());
    }

    #[test]
    fn absent_path_is_still_recorded() {
        // Reading a field that doesn't exist yet still creates a dependency, so a
        // future value invalidates the page.
        assert_eq!(
            keys(&read("#sys.inputs.baudelaire.tag")),
            ["sys.inputs.baudelaire.tag"]
        );
    }

    #[test]
    fn digest_changes_only_for_the_read_value() {
        let before = tree();
        let mut after_dict = vec![
            ("version".to_string(), Value::str("0.1.0")),
            ("date".to_string(), Value::str("2026-07-17")), // a new day
            (
                "git".to_string(),
                Value::dict([
                    ("hash", Value::str("abc123")),
                    ("dirty", Value::Bool(false)),
                ]),
            ),
        ];
        let after = Value::Dict(std::mem::take(&mut after_dict));
        let roots_before = [Root {
            base: "sys.inputs.baudelaire",
            tree: &before,
        }];
        let roots_after = [Root {
            base: "sys.inputs.baudelaire",
            tree: &after,
        }];

        // git.hash unchanged across a day boundary → same digest → no rebuild.
        assert_eq!(
            digest(&roots_before, "sys.inputs.baudelaire.git.hash"),
            digest(&roots_after, "sys.inputs.baudelaire.git.hash")
        );
        // date changed → its digest differs.
        assert_ne!(
            digest(&roots_before, "sys.inputs.baudelaire.date"),
            digest(&roots_after, "sys.inputs.baudelaire.date")
        );
    }

    #[test]
    fn absent_and_present_digests_differ() {
        let tree = tree();
        let roots = [Root {
            base: "sys.inputs.baudelaire",
            tree: &tree,
        }];
        assert_ne!(
            digest(&roots, "sys.inputs.baudelaire.missing"),
            digest(&roots, "sys.inputs.baudelaire.version")
        );
    }
}
