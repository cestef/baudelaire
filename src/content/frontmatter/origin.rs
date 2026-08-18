//! Where a page's frontmatter came from, and how a key inside it is located.

use super::Frontmatter;
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
pub struct Origin<'a> {
    pub(super) dialect: Dialect<'a>,
    pub(super) path: &'a Path,
    pub(super) collection: &'a str,
}

/// The dialect a page declared its fields in, and so how a [`Step`] path
/// resolves to a span.
pub(super) enum Dialect<'a> {
    /// `#let frontmatter = (..)` in a typst page: walk the AST.
    Typst(&'a Source),
    /// The fenced block at the top of a markdown page, in any of the dialects
    /// one may be written in, each of which resolved its own spans as it
    /// parsed.
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
    pub(crate) fn text(&self) -> &str {
        match &self.dialect {
            Dialect::Typst(source) => source.text(),
            #[cfg(feature = "markdown")]
            Dialect::Block { text, .. } => text,
        }
    }

    /// The byte span of the value `path` leads to, walking as far down as the
    /// source literally spells and underlining the deepest value reached.
    /// `None` when the frontmatter is not a dict literal this can locate, which
    /// leaves the diagnostic snippet-less.
    pub(super) fn span(&self, path: &[Step]) -> Option<SourceSpan> {
        match &self.dialect {
            Dialect::Typst(source) => Self::in_typst(source, path),
            #[cfg(feature = "markdown")]
            Dialect::Block { spans, .. } => Self::in_block(spans, path),
        }
    }

    /// Where the author wrote `key` itself, rather than the value under it.
    /// Only a top-level key, because that is the only depth at which a key is
    /// held against the known set.
    pub(crate) fn entry(&self, key: &str) -> Option<SourceSpan> {
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
    pub(crate) fn in_typst(source: &Source, path: &[Step]) -> Option<SourceSpan> {
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

    /// Look a block's span up, by the path of steps that names the value: the
    /// deepest prefix of `path` the author actually wrote.
    #[cfg(feature = "markdown")]
    pub(crate) fn in_block(
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

    /// The `#let frontmatter = ..` binding anywhere in the tree, so one
    /// declared inside a code block is located too.
    pub(super) fn binding(node: &SyntaxNode) -> Option<LetBinding<'_>> {
        if let Some(binding) = node.cast::<LetBinding>()
            && binding
                .kind()
                .bindings()
                .iter()
                .any(|ident| ident.get() == Frontmatter::EXPORT)
        {
            return Some(binding);
        }
        node.children().find_map(Self::binding)
    }
}

/// A page's frontmatter, re-read for the sake of a diagnostic by a reader that
/// holds only a path, and built only on the failing path.
pub(crate) enum Located {
    /// A typst page: the same parse the compiler holds, walked as a syntax
    /// tree.
    Typst(Source),
    /// A markdown page: its frontmatter block, re-split, with the span map its
    /// dialect recorded while parsing.
    #[cfg(feature = "markdown")]
    Block {
        text: String,
        spans: crate::content::markdown::Spans,
    },
}

impl Located {
    /// Re-read `path` far enough to locate a value inside its frontmatter.
    /// `None` when the page cannot be read or parsed, which leaves the
    /// diagnostic snippet-less rather than failing while reporting a failure.
    pub(crate) fn of(path: &Path, project: &crate::world::Project) -> Option<Self> {
        #[cfg(feature = "markdown")]
        if crate::config::Config::has_ext(path, crate::config::Config::MARKDOWN) {
            use crate::content::markdown::Document;

            let text = String::from_utf8(crate::fs::read(path).ok()?).ok()?;
            let named = path.display().to_string();
            let spans = Document::split(&text, &named)
                .ok()?
                .block(&named, &text)
                .ok()?
                .spans;
            return Some(Self::Block { text, spans });
        }
        Some(Self::Typst(project.source(path).ok()?))
    }

    /// The text a diagnostic renders its snippet from.
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Typst(source) => source.text(),
            #[cfg(feature = "markdown")]
            Self::Block { text, .. } => text,
        }
    }

    /// Where the value `path` names sits, by the same walk [`Origin::span`]
    /// does: as deep as the page literally spelled it out.
    pub(crate) fn span(&self, path: &[Step]) -> Option<SourceSpan> {
        match self {
            Self::Typst(source) => Origin::in_typst(source, path),
            #[cfg(feature = "markdown")]
            Self::Block { spans, .. } => Origin::in_block(spans, path),
        }
    }
}

/// One frontmatter key being read, and everything a diagnostic about it needs:
/// the page it is on, the source that page was written in, and where in that
/// source its value sits.
#[derive(Clone, Copy)]
pub(super) struct At<'a> {
    pub(super) origin: &'a Origin<'a>,
    pub(super) key: &'a str,
    /// Which element of a list value, when the fault is in one, so it
    /// underlines itself rather than the whole list.
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
        got: &str,
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

    /// A URL this key holds that would write outside the output directory.
    pub(super) fn traversal(self) -> BaudelaireErrorKind {
        ContentError::frontmatter_traversal(
            self.origin.path,
            self.origin.text(),
            self.span(),
            self.key,
        )
        .into()
    }

    /// A name this key does not answer to, underlined where it was written;
    /// `valid` is the set of names it does.
    pub(super) fn name(self, got: &str, valid: &[&str]) -> BaudelaireErrorKind {
        ContentError::frontmatter_name(
            self.origin.path,
            self.origin.text(),
            self.span(),
            self.key,
            got,
            &crate::config::dispatch::Keys::of(valid).help(got, "names"),
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
        assert!(origin(&computed).span(&[]).is_some());
        assert_eq!(origin(&computed).span(&[key("title")]), None);
        assert_eq!(origin(&computed).entry("titel"), None);
    }
    #[test]
    fn a_typst_page_locates_a_wrong_typed_value_and_a_typod_key() {
        let text =
            "#let frontmatter = (\n  titel: \"A\",\n  order: \"first\",\n  tags: (\"x\", 3),\n)\n";
        let source = page(text);
        let origin = origin(&source);

        let order = At::new(&origin, "order").span().expect("order's value");
        assert_eq!(cut(text, order), "\"first\"");
        let tag = At::new(&origin, "tags").nth(1).span().expect("the element");
        assert_eq!(cut(text, tag), "3");
        let titel = origin.entry("titel").expect("the key as written");
        assert_eq!(cut(text, titel), "titel");
        assert_eq!(origin.entry("absent"), None);
    }
    /// The same two on a markdown page, which reaches them through a recorded
    /// span map rather than a syntax tree.
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

        let order = At::new(&origin, "order").span().expect("order's entry");
        assert_eq!(cut(&text, order), "order: not a number");
        let tag = At::new(&origin, "tags").nth(1).span().expect("the element");
        assert_eq!(cut(&text, tag), "3");
        let titel = origin.entry("titel").expect("the key as written");
        assert_eq!(cut(&text, titel), "titel: A");
    }
}
