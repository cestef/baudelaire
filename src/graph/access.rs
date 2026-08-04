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
//! It is generic over the root value. Build a [`Roots`] set of [`Root`]s, each a
//! dotted base that names the value in source (`"sys.inputs.baudelaire"`) and
//! its current [`Value`]; [`Roots::reads`] returns the qualified paths read from
//! each, and [`Roots::digest`] fingerprints the value at one of those paths, so
//! analysis and invalidation share one source of truth and cannot drift.
//!
//! The analysis is sound by over-approximation: it never misses a read (which
//! would serve stale output), but where it cannot narrow an access (a dynamic
//! `.at(key)`, a value pulled through a destructuring) it widens to the base
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

/// The tracked-value form [`crate::world::Project::tracked`] hands out: an owned
/// `(dotted base, tree)` pair borrowed as a root. The single conversion, so no
/// consumer re-spells the field mapping.
impl<'a> From<&'a (String, Value)> for Root<'a> {
    fn from((base, tree): &'a (String, Value)) -> Self {
        Self { base, tree }
    }
}

/// The qualified value paths a source reads, e.g. `"sys.inputs.baudelaire.git.hash"`.
/// A bare base means the whole value was read (or couldn't be narrowed).
pub type Reads = BTreeSet<String>;

/// Every tracked value of one build. Analysis and invalidation both go through
/// here, so a page's recorded reads and the digests they are validated against
/// are always resolved from the same set.
pub struct Roots<'a>(Vec<Root<'a>>);

impl<'a> From<Vec<Root<'a>>> for Roots<'a> {
    fn from(roots: Vec<Root<'a>>) -> Self {
        Self(roots)
    }
}

impl<'a> FromIterator<Root<'a>> for Roots<'a> {
    fn from_iter<T: IntoIterator<Item = Root<'a>>>(roots: T) -> Self {
        Self(roots.into_iter().collect())
    }
}

impl Roots<'_> {
    /// The value paths `source` reads from these roots.
    pub fn reads(&self, source: &Source) -> Reads {
        let mut scan = Scan::new(&self.0);
        scan.walk(source.root());
        scan.out
    }

    /// The content digest of the value at a qualified `key`, or `None` when no
    /// value lives there. Two builds agree iff the digests are equal, so a path
    /// that gains or loses a value (`None` <-> `Some`) reads as a change, no
    /// sentinel needed.
    pub fn digest(&self, key: &str) -> Option<Hash> {
        self.0.iter().find(|root| root.owns(key))?.digest(key)
    }

    /// The digest of every key in `keys`, ready to store in a cache entry.
    pub fn digests(&self, keys: &Reads) -> BTreeMap<String, Option<Hash>> {
        keys.iter()
            .map(|key| (key.clone(), self.digest(key)))
            .collect()
    }

    /// Every root read whole: the widest sound answer, for a source that cannot
    /// be analyzed at all.
    fn everything(&self) -> Reads {
        self.0.iter().map(|root| root.base.to_owned()).collect()
    }
}

impl<'a> Root<'a> {
    /// Whether this root owns `key`: its base is the key, or a prefix of it.
    fn owns(&self, key: &str) -> bool {
        key == self.base
            || key
                .strip_prefix(self.base)
                .is_some_and(|r| r.starts_with('.'))
    }

    /// The qualified key an access `path` (from the global scope) reads from this
    /// root, or `None` if it doesn't touch it. A path that stops short of, equals,
    /// or grabs a non-narrowable part of the base yields the base itself (the
    /// whole value); a longer one is truncated at the first leaf it reaches, since
    /// trailing segments are method calls on the value.
    fn key(&self, path: &[String]) -> Option<String> {
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        let base: Vec<&str> = self.base.split('.').collect();
        let shared = base.len().min(path.len());
        if path[..shared] != base[..shared] {
            return None;
        }
        if path.len() <= base.len() {
            return Some(self.base.to_owned());
        }
        let mut key = self.base.to_owned();
        let mut node = self.tree;
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

    /// The value at a qualified `key` this root owns, or `None` if it's absent.
    fn value(&self, key: &str) -> Option<&'a Value> {
        let rest = key.strip_prefix(self.base)?;
        let mut node = self.tree;
        for segment in rest.strip_prefix('.').unwrap_or_default().split('.') {
            if segment.is_empty() {
                continue; // the bare base: the whole value.
            }
            node = node.get(segment)?;
        }
        Some(node)
    }

    /// The digest of the value at `key`, or `None` when the path is absent.
    fn digest(&self, key: &str) -> Option<Hash> {
        Some(Hash::of(self.value(key)?))
    }
}

/// A syntax-tree walk that accumulates the value paths a source reads. Threads
/// its state (the tracked roots, the `let`-alias environment, and the reads so
/// far) as one object rather than through every step.
struct Scan<'a> {
    roots: &'a [Root<'a>],
    /// `let` aliases into a tracked value, e.g. the `git` in
    /// `#let git = sys.inputs.baudelaire.git`, mapped to the path it stands for.
    env: HashMap<String, Vec<String>>,
    out: Reads,
}

impl<'a> Scan<'a> {
    fn new(roots: &'a [Root<'a>]) -> Self {
        Self {
            roots,
            env: HashMap::new(),
            out: Reads::new(),
        }
    }

    /// Walk a node, recording the read of every maximal access. Recording happens
    /// at the outermost resolvable node, then descends only into call arguments
    /// (never the callee chain), so a chain is recorded once, not per link.
    fn walk(&mut self, node: &SyntaxNode) {
        if let Some(expr) = node.cast::<Expr>() {
            // A binding we can alias is not a read: the value becomes a dependency
            // where the alias is *used*, so we only scan a call's arguments (a
            // default is still a read). A binding we can't alias (a destructuring
            // or complex pattern) would let the value escape untracked, so its
            // initializer is recorded like any other use instead.
            if let Expr::LetBinding(binding) = expr {
                let init = binding.init().map(Expr::to_untyped);
                match (self.bind(binding), init) {
                    (true, Some(init)) => self.args(init),
                    (false, Some(init)) => self.walk(init),
                    (_, None) => {}
                }
                return;
            }
            if let Some(path) = self.resolve(&expr) {
                self.record(&path);
                self.args(node);
                return;
            }
        }
        for child in node.children() {
            self.walk(child);
        }
    }

    /// Walk the argument list of a call node (a no-op for anything else), so a
    /// read nested in an argument is caught while the callee chain is left alone.
    fn args(&mut self, node: &SyntaxNode) {
        if let Some(Expr::FuncCall(call)) = node.cast::<Expr>() {
            for arg in call.args().items() {
                self.walk(arg.to_untyped());
            }
        }
    }

    /// Alias `#let name = <access>` so later uses of `name` resolve through it,
    /// returning whether an alias was created. Only a plain `name = access` binds;
    /// a destructuring or complex pattern, or a non-access initializer, returns
    /// `false` so the caller records the initializer instead of dropping a value
    /// it can't follow.
    fn bind(&mut self, binding: ast::LetBinding) -> bool {
        let ast::LetBindingKind::Normal(ast::Pattern::Normal(Expr::Ident(name))) = binding.kind()
        else {
            return false;
        };
        let Some(path) = binding.init().and_then(|init| self.resolve(&init)) else {
            return false;
        };
        self.env.insert(name.get().to_string(), path);
        true
    }

    /// Resolve an access to its path from the global scope, or `None` if it isn't
    /// a plain access. Follows identifiers (through `env` aliases), field access,
    /// `.at("key")`, and the collection methods that expose a whole value.
    fn resolve(&self, expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Ident(ident) => {
                let name = ident.get();
                Some(
                    self.env
                        .get(name.as_str())
                        .cloned()
                        .unwrap_or_else(|| vec![name.to_string()]),
                )
            }
            Expr::FieldAccess(access) => {
                let mut path = self.resolve(&access.target())?;
                path.push(access.field().get().to_string());
                Some(path)
            }
            Expr::FuncCall(call) => self.call(*call),
            Expr::Parenthesized(paren) => self.resolve(&paren.expr()),
            _ => None,
        }
    }

    /// Resolve `<target>.method(..)` for the accessors that read a value: `.at("k")`
    /// narrows to that key; a dynamic `.at(expr)` and the collection accessors
    /// (`.keys`, `.values`, ..) widen to the whole target.
    fn call(&self, call: ast::FuncCall) -> Option<Vec<String>> {
        let Expr::FieldAccess(access) = call.callee() else {
            return None;
        };
        let mut path = self.resolve(&access.target())?;
        match access.field().get().as_str() {
            "at" => {
                let first = call.args().items().find_map(|arg| match arg {
                    ast::Arg::Pos(expr) => Some(expr),
                    _ => None,
                });
                if let Some(Expr::Str(key)) = first {
                    path.push(key.get().to_string());
                }
                Some(path)
            }
            "keys" | "values" | "pairs" | "len" | "has" | "contains" => Some(path),
            _ => None,
        }
    }

    /// Record the qualified key `path` reads from each root it touches.
    fn record(&mut self, path: &[String]) {
        for root in self.roots {
            if let Some(key) = root.key(path) {
                self.out.insert(key);
            }
        }
    }
}

/// A build-scoped analyzer: the tracked roots plus a per-file memo, so a template
/// shared by hundreds of pages is analyzed once, not once per page.
pub struct Analyzer<'a> {
    roots: Roots<'a>,
    project: &'a Project,
    memo: Mutex<HashMap<PathBuf, Arc<Reads>>>,
}

impl<'a> Analyzer<'a> {
    /// Build an analyzer over `roots` for the pages of `project`.
    pub fn new(roots: impl Into<Roots<'a>>, project: &'a Project) -> Self {
        Self {
            roots: roots.into(),
            project,
            memo: Mutex::new(HashMap::new()),
        }
    }

    /// The roots this analyzer resolves against, for a caller that has to digest
    /// what [`reads`](Analyzer::reads) reported. Borrowed rather than rebuilt,
    /// so the keys a cache records and the digests it checks them against come
    /// from one set.
    pub fn roots(&self) -> Roots<'_> {
        Roots(self.roots.0.clone())
    }

    /// The value paths a page reads: its own compiled source (which carries any
    /// inline generated body) unioned with every `.typ` file it depends on
    /// (templates, imported modules, its `#include`d body).
    pub fn reads(&self, source: &Source, deps: &Deps) -> Reads {
        let mut out = self.roots.reads(source);
        for path in deps.files() {
            if path.extension().is_some_and(|ext| ext == "typ") {
                out.extend(self.file(path).iter().cloned());
            }
        }
        out
    }

    /// The reads of a dependency file, analyzed once per build.
    fn file(&self, path: &Path) -> Arc<Reads> {
        if let Some(cached) = self.memo.lock().get(path) {
            return Arc::clone(cached);
        }
        let found = match self.project.source(path) {
            Ok(source) => self.roots.reads(&source),
            // A file that could not be loaded must not be recorded as reading
            // *nothing*: that is the unsound direction, and it leaves a page
            // depending on a dependency it can never be invalidated by (a
            // `@preview` theme in the package cache reading `git.hash` would go
            // stale across every commit). Widen to every root instead, the same
            // over-approximation this analysis uses for an unnarrowable access.
            Err(_) => self.roots.everything(),
        };
        let found = Arc::new(found);
        self.memo.lock().insert(path.to_owned(), Arc::clone(&found));
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

    fn roots(tree: &Value) -> Roots<'_> {
        Roots::from(vec![Root {
            base: "sys.inputs.baudelaire",
            tree,
        }])
    }

    fn read(code: &str) -> Reads {
        let source = Source::detached(code);
        let tree = tree();
        roots(&tree).reads(&source)
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
        let code = r"
            #let build = sys.inputs.baudelaire
            #build.version
        ";
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
    fn transitive_aliases_chain() {
        // a = root; c = a; d = c; then read a leaf off d.
        let code = r"
            #let a = sys.inputs.baudelaire
            #let c = a
            #let d = c
            #d.git.hash
        ";
        assert_eq!(keys(&read(code)), ["sys.inputs.baudelaire.git.hash"]);
    }

    #[test]
    fn destructuring_the_value_widens_soundly() {
        // We can't alias `git` through a destructuring pattern, so the whole
        // value it's pulled from is recorded, never dropped (that would be a
        // stale-output bug).
        let code = "#let (git,) = sys.inputs.baudelaire\n#git.hash";
        assert_eq!(keys(&read(code)), ["sys.inputs.baudelaire"]);
    }

    #[test]
    fn destructuring_a_tuple_of_leaves_stays_precise() {
        // The initializer is an array of individual reads, so each is recorded
        // on its own, no need to widen to the whole context.
        let code = "#let (v, d) = (sys.inputs.baudelaire.version, sys.inputs.baudelaire.date)";
        assert_eq!(
            keys(&read(code)),
            [
                "sys.inputs.baudelaire.date",
                "sys.inputs.baudelaire.version"
            ]
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
        let after = Value::dict([
            ("version", Value::str("0.1.0")),
            ("date", Value::str("2026-07-17")), // a new day
            (
                "git",
                Value::dict([
                    ("hash", Value::str("abc123")),
                    ("dirty", Value::Bool(false)),
                ]),
            ),
        ]);
        let (before, after) = (roots(&before), roots(&after));

        // git.hash unchanged across a day boundary -> same digest -> no rebuild.
        assert_eq!(
            before.digest("sys.inputs.baudelaire.git.hash"),
            after.digest("sys.inputs.baudelaire.git.hash")
        );
        // date changed -> its digest differs.
        assert_ne!(
            before.digest("sys.inputs.baudelaire.date"),
            after.digest("sys.inputs.baudelaire.date")
        );
    }

    #[test]
    fn absent_and_present_digests_differ() {
        let tree = tree();
        let roots = roots(&tree);
        assert_ne!(
            roots.digest("sys.inputs.baudelaire.missing"),
            roots.digest("sys.inputs.baudelaire.version")
        );
    }
}
