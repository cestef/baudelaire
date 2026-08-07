//! Where a page's frontmatter came from, and how a key inside it is located.

use super::check::Step;
use crate::error::{BaudelaireErrorKind, ContentError};
use miette::SourceSpan;
use std::path::Path;
use typst::syntax::{
    Source, SyntaxNode,
    ast::{ArrayItem, AstNode, DictItem, Expr, LetBinding},
};
/// Where a frontmatter dict came from: the source its spans point into, the
/// path errors name it by, and the collection whose schema constrains it.
///
/// One value rather than three parameters threaded through extraction, because
/// every one of them exists to make a diagnostic precise and they are always
/// needed together.
pub struct Origin<'a> {
    pub(super) dialect: Dialect<'a>,
    pub(super) path: &'a Path,
    pub(super) collection: &'a str,
}

/// The dialect a page declared its fields in, and so how a [`Step`] path
/// resolves to a span.
///
/// Both reach the same [`Frontmatter`] through the same [`FIELDS`] walk; they
/// differ only in what the author actually wrote, and a diagnostic has to
/// underline that rather than a reconstruction of it. A markdown page has no
/// typst `Source` at all, which is why this is a dialect and not an `Option`.
pub(super) enum Dialect<'a> {
    /// `#let frontmatter = (..)` in a typst page: walk the AST.
    Typst(&'a Source),
    /// The fenced block at the top of a markdown page, in any of the dialects
    /// one may be written in. One arm rather than three: each dialect resolved
    /// its own document into file-relative spans as it parsed, so by here they
    /// are indistinguishable.
    #[cfg(feature = "markdown")]
    Block {
        text: &'a str,
        spans: &'a crate::content::markdown::Spans,
    },
}
impl<'a> Origin<'a> {
    pub fn new(source: &'a Source, path: &'a Path, collection: &'a str) -> Self {
        Self {
            dialect: Dialect::Typst(source),
            path,
            collection,
        }
    }

    /// A markdown page's frontmatter block, whichever dialect it was written
    /// in. `text` is the whole file, not the block, so the recorded spans
    /// underline the right line of the snippet.
    #[cfg(feature = "markdown")]
    pub fn block(
        text: &'a str,
        spans: &'a crate::content::markdown::Spans,
        path: &'a Path,
        collection: &'a str,
    ) -> Self {
        Self {
            dialect: Dialect::Block { text, spans },
            path,
            collection,
        }
    }

    /// The text a diagnostic renders its snippet from.
    pub(super) fn text(&self) -> &str {
        match &self.dialect {
            Dialect::Typst(source) => source.text(),
            #[cfg(feature = "markdown")]
            Dialect::Block { text, .. } => text,
        }
    }

    /// The byte span of the value `path` leads to inside
    /// `#let frontmatter = (..)`, or of the binding itself for the empty path: a
    /// field that is absent has nowhere of its own to point at.
    ///
    /// Walks as far down `path` as the source literally spells, and underlines
    /// the deepest value it reached. A nested key the page never wrote stops one
    /// step short, at the dictionary that should have held it, which is where a
    /// reader would go to add it.
    ///
    /// `None` when the frontmatter is not a dict literal this can locate (it
    /// may be computed, or imported), which leaves the diagnostic snippet-less
    /// rather than underlining an arbitrary offset.
    pub(super) fn span(&self, path: &[Step]) -> Option<SourceSpan> {
        match &self.dialect {
            Dialect::Typst(source) => Self::in_typst(source, path),
            #[cfg(feature = "markdown")]
            Dialect::Block { spans, .. } => Self::in_block(spans, path),
        }
    }

    /// Where the author wrote `key` itself, rather than the value under it:
    /// what an unrecognized key underlines, since the mistake is the key.
    ///
    /// Only a top-level key, because that is the only depth at which a key is
    /// held against the known set. A block dialect records an entry from its
    /// key where its syntax has one to start from, so its recorded span is
    /// already the closest thing to the key it has.
    pub(super) fn entry(&self, key: &str) -> Option<SourceSpan> {
        match &self.dialect {
            Dialect::Typst(source) => {
                let Expr::Dict(dict) = Self::binding(source.root())?.init()? else {
                    return None;
                };
                let name = dict.items().find_map(|item| match item {
                    DictItem::Named(named) if named.name().get() == key => Some(named.name()),
                    _ => None,
                })?;
                Self::locate(source, name.to_untyped())
            }
            #[cfg(feature = "markdown")]
            Dialect::Block { spans, .. } => Self::in_block(spans, &[Step::Key(key.to_owned())]),
        }
    }

    /// Walk a typst dict literal.
    pub(super) fn in_typst(source: &Source, path: &[Step]) -> Option<SourceSpan> {
        let binding = Self::binding(source.root())?;
        let mut node = binding.to_untyped();
        let mut reached = 0;
        if let Some(mut expr) = binding.init() {
            for step in path {
                let Some(next) = Self::descend(expr, step) else {
                    break;
                };
                node = next.to_untyped();
                expr = next;
                reached += 1;
            }
        }
        // A path that got nowhere is a frontmatter this cannot read into at all
        // (computed, or imported): underlining the binding would label the whole
        // of it as the value that is wrong.
        if !path.is_empty() && reached == 0 {
            return None;
        }
        Self::locate(source, node)
    }

    /// A node of the page's syntax tree, as a span into the page's text.
    pub(super) fn locate(source: &Source, node: &SyntaxNode) -> Option<SourceSpan> {
        let range = source.find(node.span())?.range();
        Some(SourceSpan::new(range.start.into(), range.len()))
    }

    /// Look a block's span up, by the path of steps that names the value.
    ///
    /// Every dialect resolved its own document into a
    /// [`Spans`](crate::content::markdown::Spans) as it parsed, so there is one
    /// of these rather than one walk per language, and the rule is
    /// [`Spans::of`]'s: the deepest prefix of `path` the author actually wrote.
    #[cfg(feature = "markdown")]
    pub(super) fn in_block(
        spans: &crate::content::markdown::Spans,
        path: &[Step],
    ) -> Option<SourceSpan> {
        let steps: Vec<String> = path.iter().map(ToString::to_string).collect();
        let span = spans.of(&steps)?;
        Some(SourceSpan::new(span.start.into(), span.len()))
    }

    /// One step into a literal: a named dict item, or a positional array
    /// element. Anything else (a spread, a computed key, a call) is not a
    /// literal this can point inside of, and stops the walk.
    pub(super) fn descend<'b>(expr: Expr<'b>, step: &Step) -> Option<Expr<'b>> {
        match (expr, step) {
            (Expr::Dict(dict), Step::Key(key)) => dict.items().find_map(|item| match item {
                DictItem::Named(named) if named.name().get() == key => Some(named.expr()),
                _ => None,
            }),
            (Expr::Array(array), Step::Index(i)) => match array.items().nth(*i)? {
                ArrayItem::Pos(expr) => Some(expr),
                ArrayItem::Spread(_) => None,
            },
            _ => None,
        }
    }

    /// The `#let frontmatter = ..` binding anywhere in the tree, so a page that
    /// declares it inside a code block is located just as well as the
    /// conventional top-level form.
    pub(super) fn binding(node: &SyntaxNode) -> Option<LetBinding<'_>> {
        if let Some(binding) = node.cast::<LetBinding>()
            && binding
                .kind()
                .bindings()
                .iter()
                .any(|ident| ident.get() == "frontmatter")
        {
            return Some(binding);
        }
        node.children().find_map(Self::binding)
    }
}

/// One frontmatter key being read, and everything a diagnostic about it needs:
/// the page it is on, the source that page was written in, and where in that
/// source its value sits.
///
/// One value rather than the `(path, key)` pair the accessors used to take.
/// That pair could name the file and the field but not point at either, so
/// every wrong-typed value and every typo'd key reported itself with no snippet
/// at all. The loop over the dict is the one place holding both the [`Origin`]
/// and the key, and this is what carries them the rest of the way down.
#[derive(Clone, Copy)]
pub(super) struct At<'a> {
    pub(super) origin: &'a Origin<'a>,
    pub(super) key: &'a str,
    /// Which element of a list value, when the fault is in one: a wrong-typed
    /// element underlines itself rather than the whole list around it.
    pub(super) element: Option<usize>,
}
impl<'a> At<'a> {
    pub(super) fn new(origin: &'a Origin<'a>, key: &'a str) -> Self {
        Self {
            origin,
            key,
            element: None,
        }
    }

    /// The same key, narrowed to one element of the list it holds.
    pub(super) fn nth(self, index: usize) -> Self {
        Self {
            element: Some(index),
            ..self
        }
    }

    /// Where this value sits, as far down as the page literally spelled it out.
    pub(super) fn span(self) -> Option<SourceSpan> {
        let mut steps = vec![Step::Key(self.key.to_owned())];
        steps.extend(self.element.map(Step::Index));
        self.origin.span(&steps)
    }

    /// A value that is not the type its key holds.
    pub(super) fn field(
        self,
        expected: &'static str,
        got: &'static str,
        help: Option<&'static str>,
    ) -> BaudelaireErrorKind {
        ContentError::frontmatter_field(
            self.origin.path,
            self.origin.text(),
            self.span(),
            self.key,
            expected,
            got,
            help,
        )
        .into()
    }

    /// A name this key does not answer to, underlined where it was written.
    ///
    /// `valid` is the list of names it does, built by the caller from the very
    /// table that parses them.
    pub(super) fn name(self, got: &str, valid: &str) -> BaudelaireErrorKind {
        ContentError::frontmatter_name(
            self.origin.path,
            self.origin.text(),
            self.span(),
            self.key,
            got,
            valid,
        )
        .into()
    }

    /// A key that is a near-miss of a known one, underlined where it was
    /// written rather than at the value it carries.
    pub(super) fn unknown(self, suggestion: &str) -> BaudelaireErrorKind {
        ContentError::unknown_frontmatter(
            self.origin.path,
            self.origin.text(),
            self.origin.entry(self.key),
            self.key,
            suggestion,
        )
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::{At, Origin};
    use crate::content::frontmatter::check::Step;
    use miette::SourceSpan;
    use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};

    fn key(name: &str) -> Step {
        Step::Key(name.to_owned())
    }
    /// What a span underlines, which is the only thing any of these assert on.
    fn cut(text: &str, span: SourceSpan) -> &str {
        &text[span.offset()..span.offset() + span.len()]
    }
    fn page(text: &str) -> Source {
        let path = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("page.typ").expect("a valid vpath"),
        );
        Source::new(FileId::unique(path), text.into())
    }
    fn origin(source: &Source) -> Origin<'_> {
        Origin::new(source, std::path::Path::new("page.typ"), "blog")
    }
    /// The span is what makes a schema failure readable, and it is the one part
    /// the build never exercises on a green run: a locator that silently
    /// returned `None` would leave every one of these diagnostics snippet-less.
    #[test]
    fn a_key_locates_its_own_value_and_a_missing_one_the_binding() {
        let text = "#let frontmatter = (\n  title: \"Hello\",\n  hero: 3,\n)\n\nBody.\n";
        let source = page(text);
        let origin = origin(&source);

        let hero = origin.span(&[key("hero")]).expect("hero has a value");
        assert_eq!(&text[hero.offset()..hero.offset() + hero.len()], "3");
        let title = origin.span(&[key("title")]).expect("title has a value");
        assert_eq!(
            &text[title.offset()..title.offset() + title.len()],
            "\"Hello\""
        );
        // A key that is not there points at the binding, which is the thing
        // that should have carried it.
        // The binding node starts at `let`: in markup the `#` is a token of
        // its own, ahead of the expression.
        let binding = origin.span(&[]).expect("the binding");
        assert!(text[binding.offset()..].starts_with("let frontmatter"));
        assert_eq!(origin.span(&[key("absent")]), None);
    }
    /// A nested path stops at the deepest value the page actually wrote, which
    /// is where the field it is missing would go.
    #[test]
    fn a_nested_key_locates_the_value_or_the_dict_that_should_hold_it() {
        let text = "#let frontmatter = (\n  authors: ((name: \"A\"), (name: 2)),\n)\n";
        let source = page(text);
        let origin = origin(&source);

        let path = [key("authors"), Step::Index(1), key("name")];
        let name = origin.span(&path).expect("the second author's name");
        assert_eq!(&text[name.offset()..name.offset() + name.len()], "2");

        let absent = [key("authors"), Step::Index(0), key("email")];
        let dict = origin.span(&absent).expect("the dict that should hold it");
        assert_eq!(
            &text[dict.offset()..dict.offset() + dict.len()],
            "(name: \"A\")"
        );
    }
    /// A binding the locator cannot read leaves the diagnostic snippet-less
    /// rather than underlining an arbitrary offset.
    #[test]
    fn a_frontmatter_that_is_not_a_dict_literal_locates_nothing() {
        let imported = page("#import \"meta.typ\": frontmatter\n");
        assert_eq!(origin(&imported).span(&[]), None);

        let computed = page("#let frontmatter = build()\n");
        // The binding is still where it is; only the key inside it is not.
        assert!(origin(&computed).span(&[]).is_some());
        assert_eq!(origin(&computed).span(&[key("title")]), None);
        // Neither is the key it never spelled out, so a typo in a computed
        // frontmatter is reported without a snippet rather than with a wrong one.
        assert_eq!(origin(&computed).entry("titel"), None);
    }
    /// The two frontmatter errors a page hits before any schema does: a
    /// wrong-typed value and a typo'd key. Both are only readable with a
    /// snippet, and these are what decides whether they get one.
    #[test]
    fn a_typst_page_locates_a_wrong_typed_value_and_a_typod_key() {
        let text =
            "#let frontmatter = (\n  titel: \"A\",\n  order: \"first\",\n  tags: (\"x\", 3),\n)\n";
        let source = page(text);
        let origin = origin(&source);

        let order = At::new(&origin, "order").span().expect("order's value");
        assert_eq!(cut(text, order), "\"first\"");
        // A wrong-typed element of a list underlines itself, not the list.
        let tag = At::new(&origin, "tags").nth(1).span().expect("the element");
        assert_eq!(cut(text, tag), "3");
        // The key, not the value under it: the mistake is the key.
        let titel = origin.entry("titel").expect("the key as written");
        assert_eq!(cut(text, titel), "titel");
        assert_eq!(origin.entry("absent"), None);
    }
    /// The same two on a markdown page, which reaches them through a recorded
    /// span map rather than a syntax tree: one walk per dialect would be one
    /// place for the snippet to go missing.
    #[cfg(feature = "markdown")]
    #[test]
    fn a_markdown_page_locates_the_same_two_things() {
        use crate::content::markdown::Dialect;

        let block = "titel: A\norder: not a number\ntags:\n  - x\n  - 3\n";
        let text = format!("---\n{block}---\n\nBody.\n");
        let spans = Dialect::Yaml
            .parse(block, 4, "page.md", &text)
            .expect("valid YAML")
            .spans;
        let path = std::path::Path::new("page.md");
        let origin = Origin::block(&text, &spans, path, "blog");

        // A block dialect records an entry from its key, so the value span
        // covers the line rather than half of it.
        let order = At::new(&origin, "order").span().expect("order's entry");
        assert_eq!(cut(&text, order), "order: not a number");
        let tag = At::new(&origin, "tags").nth(1).span().expect("the element");
        assert_eq!(cut(&text, tag), "3");
        let titel = origin.entry("titel").expect("the key as written");
        assert_eq!(cut(&text, titel), "titel: A");
    }
}
