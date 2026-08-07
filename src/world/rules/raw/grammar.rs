//! What a code block's `lang` resolves to, and the classed pieces it yields.

use std::ops::Range;
use std::sync::Arc;

use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, ScopeStack, SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};
use typst::diag::SourceResult;
use typst::ecow::EcoString;
use typst::engine::Engine;
use typst::foundations::{Bytes, Packed, Smart, StyleChain};
use typst::loading::Load;
use typst::syntax::{LinkedNode, Spanned, Tag, split_newlines};
use typst::text::{RAW_SYNTAXES, RawElem};

use crate::config::Token;

use super::scope::Scopes;

/// One classed piece of a highlighted block, in document order.
pub(super) struct Piece<'a> {
    /// Which of the block's lines it falls on.
    pub(super) line: usize,
    /// The text itself, never empty and never spanning a line break.
    pub(super) text: &'a str,
    /// Its byte offset within that line, which is what typst turns back into a
    /// source position when a diagnostic points inside a code block.
    pub(super) offset: usize,
    /// The mark it carries: the vocabulary entry it resolved to and the
    /// grammar's own scope name, or `None` for text the vocabulary does not
    /// name. Both, because a site may ask to keep the scope beside the class,
    /// and this is the only place that still knows it.
    pub(super) token: Option<(Token, EcoString)>,
}

/// How typst parses a block of its own code, per the language tag.
#[derive(Debug, Clone, Copy)]
pub(super) enum Mode {
    Markup,
    Code,
    Math,
}

/// Where a resolved grammar's syntax set lives: typst's bundled one, or one the
/// page loaded itself. A borrow of either would tie the [`Grammar`] to it, so
/// the loaded case owns its `Arc` and both answer [`Set::get`].
pub(super) enum Set {
    Builtin,
    Loaded(Arc<SyntaxSet>),
}

/// The highlighter a code block resolved to.
pub(super) enum Grammar {
    /// typst's own parser, for `typ`, `typc` and `typm`.
    Typst(Mode),
    /// A sublime grammar, named by the language token that found it. Kept as a
    /// token rather than a `&SyntaxReference` for the same borrow reason
    /// [`Set`] exists; the second lookup is a hash map hit.
    Sublime(Set, EcoString),
    /// No grammar, or highlighting the author turned off: every line is one
    /// unclassed piece.
    Plain,
}

impl Set {
    /// The set to highlight against.
    fn get(&self) -> &SyntaxSet {
        match self {
            Self::Builtin => &RAW_SYNTAXES,
            Self::Loaded(set) => set,
        }
    }

    /// A sublime grammar's own one-syntax set, or `None` when the bytes will not
    /// parse.
    ///
    /// typst decodes the same bytes into a `RawSyntax` and keeps the
    /// [`SyntaxSet`] to itself, so decoding them again is the only way to one
    /// here. Memoized on the bytes, so a site whose template names a grammar
    /// once builds it once, however many pages that `set` rule reaches.
    ///
    /// A failure is silent because it cannot be new: these bytes already parsed
    /// when the `set` rule loaded them, and typst reported it there if they did
    /// not. Leaving the block unhighlighted beats failing a page the compiler
    /// itself accepted.
    #[comemo::memoize]
    fn decode(bytes: &Bytes) -> Option<Arc<SyntaxSet>> {
        let definition = SyntaxDefinition::load_from_str(bytes.as_str().ok()?, false, None).ok()?;
        let mut builder = SyntaxSetBuilder::new();
        builder.add(definition);
        Some(Arc::new(builder.build()))
    }
}

impl Grammar {
    /// What `elem` should be highlighted with, resolved the way typst resolves
    /// it: typst's own languages first, then the grammars the page loaded, then
    /// the bundled set.
    pub(super) fn of(
        elem: &Packed<RawElem>,
        engine: &mut Engine,
        styles: StyleChain,
    ) -> SourceResult<Self> {
        // `theme: none` is an author saying "do not highlight this block". The
        // theme means nothing else here, since no colour of it reaches the page.
        if matches!(elem.theme.get_ref(styles), Smart::Custom(None)) {
            return Ok(Self::Plain);
        }

        let lang = elem
            .lang
            .get_ref(styles)
            .as_ref()
            .map_or_else(|| EcoString::from("txt"), EcoString::to_lowercase);

        match lang.as_str() {
            "typ" | "typst" => return Ok(Self::Typst(Mode::Markup)),
            "typc" => return Ok(Self::Typst(Mode::Code)),
            "typm" => return Ok(Self::Typst(Mode::Math)),
            _ => {}
        }

        for set in Self::loaded(elem, engine, styles)? {
            if set.get().find_syntax_by_token(&lang).is_some() {
                return Ok(Self::Sublime(set, lang));
            }
        }
        Ok(match RAW_SYNTAXES.find_syntax_by_token(&lang) {
            Some(_) => Self::Sublime(Set::Builtin, lang),
            None => Self::Plain,
        })
    }

    /// Split `lines` into classed pieces and hand each to `piece`, in order.
    ///
    /// `lines` are the block's lines as typst preprocessed them: tabs expanded,
    /// no line breaks left inside one. Taking them rather than the element's own
    /// text is what keeps this rule out of the business of reproducing that.
    pub(super) fn tokens(&self, lines: &[EcoString], piece: &mut dyn FnMut(Piece<'_>)) {
        match self {
            Self::Typst(mode) => Self::typst(*mode, lines, piece),
            Self::Sublime(set, lang) => Self::sublime(set.get(), lang, lines, piece),
            Self::Plain => Self::plain(lines, piece),
        }
    }

    /// The syntax sets this page loaded through `raw(syntaxes: ..)`, in the
    /// order it named them.
    fn loaded(
        elem: &Packed<RawElem>,
        engine: &mut Engine,
        styles: StyleChain,
    ) -> SourceResult<Vec<Set>> {
        let sources = elem.syntaxes.get_cloned(styles).source;
        if sources.0.is_empty() {
            return Ok(Vec::new());
        }
        // The world has these bytes memoized from the `set` rule's own load, so
        // this reads no file a second time.
        let loaded = Spanned::new(sources, elem.span()).load(engine.world)?;
        Ok(loaded
            .iter()
            .filter_map(|data| Set::decode(&data.data))
            .map(Set::Loaded)
            .collect())
    }

    /// Every line as one unclassed piece.
    fn plain(lines: &[EcoString], piece: &mut dyn FnMut(Piece<'_>)) {
        for (line, text) in lines.iter().enumerate() {
            piece(Piece {
                line,
                text,
                offset: 0,
                token: None,
            });
        }
    }

    /// Highlight through a sublime grammar, one line at a time, carrying the
    /// scope stack across lines so a block comment or a here-doc keeps its
    /// scope past the line it opened on.
    fn sublime(set: &SyntaxSet, lang: &str, lines: &[EcoString], piece: &mut dyn FnMut(Piece<'_>)) {
        let Some(syntax) = set.find_syntax_by_token(lang) else {
            return Self::plain(lines, piece);
        };
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        for (line, text) in lines.iter().enumerate() {
            // Without the line break, which is what these grammars expect:
            // typst's bundled set and a `.sublime-syntax` this decodes are both
            // built in syntect's no-newline mode, where a rule that would have
            // matched the break matches the end of the line instead. Handing one
            // over anyway leaves a line's first token unrecognised.
            let Ok(ops) = state.parse_line(text, set) else {
                // A grammar that cannot parse its own language (a runaway regex,
                // a context that recurses) is typst's report to make, not this
                // rule's: print the line and carry on.
                piece(Piece {
                    line,
                    text,
                    offset: 0,
                    token: None,
                });
                continue;
            };

            let mut offset = 0;
            for (region, op) in ScopeRegionIterator::new(&ops, text) {
                if stack.apply(op).is_err() {
                    break;
                }
                if region.is_empty() {
                    continue;
                }
                let scopes = Scopes(stack.as_slice());
                piece(Piece {
                    line,
                    text: region,
                    offset,
                    token: scopes.token().map(|token| (token, scopes.name().into())),
                });
                offset += region.len();
            }
        }
    }

    /// Highlight typst's own code: parse it as one document, then walk the tree
    /// and slice each leaf back over the lines it covers.
    fn typst(mode: Mode, lines: &[EcoString], piece: &mut dyn FnMut(Piece<'_>)) {
        let text = lines
            .iter()
            .map(EcoString::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        let root = match mode {
            Mode::Markup => typst::syntax::parse(&text),
            Mode::Code => typst::syntax::parse_code(&text),
            Mode::Math => typst::syntax::parse_math(&text),
        };

        // Where each line starts in `text`. Joined with `\n` just above, so a
        // line is exactly one byte from the end of the one before it, whatever
        // the file's own line endings were.
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0;
        for line in lines {
            starts.push(at);
            at += line.len() + 1;
        }

        Self::leaves(&LinkedNode::new(&root), None, &mut |range, tag| {
            // typst's own tag needs no scope round-trip to become a token, but
            // it names the scope it stands for, which is what a `data-scope`
            // stamp carries here.
            let token = tag.map(|tag| (Token::from(tag), EcoString::from(tag.tm_scope())));
            // The line the leaf opens on; every part after a break is the next.
            let opens = starts.partition_point(|&start| start <= range.start) - 1;
            let mut at = range.start;
            for (line, part) in (opens..).zip(split_newlines(&text[range])) {
                if !part.is_empty() {
                    piece(Piece {
                        line,
                        text: part,
                        offset: at - starts[line],
                        token: token.clone(),
                    });
                }
                at += part.len() + 1;
            }
        });
    }

    /// Visit every leaf of `node`, carrying the innermost tag seen on the way
    /// down.
    ///
    /// typst stacks the tags instead and lets a theme's selectors pick, which
    /// comes to the same thing for the themes it ships: the deepest tag is the
    /// most specific scope. Here it is the answer directly, with no scope
    /// spelled anywhere in between.
    fn leaves(
        node: &LinkedNode,
        tag: Option<Tag>,
        leaf: &mut impl FnMut(Range<usize>, Option<Tag>),
    ) {
        let tag = typst::syntax::highlight(node).or(tag);
        if node.children().len() == 0 {
            leaf(node.range(), tag);
            return;
        }
        for child in node.children() {
            Self::leaves(&child, tag, leaf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Grammar, Mode, Set};
    use crate::config::Named;
    use typst::ecow::EcoString;

    /// Every piece a grammar yields, as `(line, offset, text, class)`.
    fn pieces(grammar: &Grammar, lines: &[&str]) -> Vec<(usize, usize, String, String)> {
        let lines: Vec<EcoString> = lines.iter().map(|l| EcoString::from(*l)).collect();
        let mut out = Vec::new();
        grammar.tokens(&lines, &mut |piece| {
            out.push((
                piece.line,
                piece.offset,
                piece.text.to_owned(),
                piece
                    .token
                    .map(|(token, _)| token.name().to_owned())
                    .unwrap_or_default(),
            ));
        });
        out
    }

    /// What a stylesheet sees: the classed pieces alone.
    fn classed(grammar: &Grammar, lines: &[&str]) -> Vec<(String, String)> {
        pieces(grammar, lines)
            .into_iter()
            .filter(|(_, _, _, class)| !class.is_empty())
            .map(|(_, _, text, class)| (text, class))
            .collect()
    }

    #[test]
    fn a_plain_block_is_one_piece_per_line() {
        assert_eq!(
            pieces(&Grammar::Plain, &["a b", "c"]),
            vec![
                (0, 0, "a b".to_owned(), String::new()),
                (1, 0, "c".to_owned(), String::new()),
            ]
        );
    }

    /// The pieces of a line concatenate back to the line, at the offsets they
    /// claim: a piece dropped or an offset off by one is markup that no longer
    /// says what the author wrote.
    #[test]
    fn the_pieces_of_a_line_rebuild_it() {
        let lines = ["fn main() {", "    let x = 1; // one", "}"];
        for grammar in [
            Grammar::Sublime(Set::Builtin, "rust".into()),
            Grammar::Plain,
        ] {
            let mut rebuilt = vec![String::new(); lines.len()];
            for (line, offset, text, _) in pieces(&grammar, &lines) {
                assert_eq!(offset, rebuilt[line].len(), "{text:?} lands where it says");
                rebuilt[line].push_str(&text);
            }
            assert_eq!(rebuilt, lines);
        }
    }

    #[test]
    fn a_sublime_grammar_classes_its_keywords() {
        let classed = classed(
            &Grammar::Sublime(Set::Builtin, "rust".into()),
            &["let x = \"a\"; // hi"],
        );
        assert!(classed.contains(&("let".to_owned(), "keyword".to_owned())));
        assert!(
            classed
                .iter()
                .any(|(text, class)| text.contains('a') && class == "string")
        );
        assert!(
            classed
                .iter()
                .any(|(text, class)| text.contains("hi") && class == "comment")
        );
    }

    /// A scope opened on one line and closed on another has to survive the line
    /// break, which is the whole reason the parse state outlives a line.
    #[test]
    fn a_block_comment_stays_a_comment_across_lines() {
        let classed = classed(
            &Grammar::Sublime(Set::Builtin, "rust".into()),
            &["/* one", "two */ let"],
        );
        assert!(classed.iter().any(|(text, class)| text.contains("two")
            && class == "comment"
            && !text.contains("let")));
    }

    /// A language nothing knows still prints, unclassed.
    #[test]
    fn an_unknown_language_falls_back_to_plain() {
        assert_eq!(
            classed(&Grammar::Sublime(Set::Builtin, "nosuchlang".into()), &["x"]),
            Vec::new()
        );
    }

    #[test]
    fn typst_code_classes_through_its_own_parser() {
        let classed = classed(&Grammar::Typst(Mode::Markup), &["#let x = 1", "= Heading"]);
        assert!(classed.contains(&("let".to_owned(), "keyword".to_owned())));
        assert!(classed.contains(&("1".to_owned(), "number".to_owned())));
        assert!(
            classed
                .iter()
                .any(|(text, class)| text.contains("Heading") && class == "heading")
        );
    }

    /// A multi-line string is one leaf of the tree covering three lines, which
    /// is the case that has to be sliced back over them.
    #[test]
    fn a_leaf_spanning_lines_is_split_at_each_break() {
        let lines = ["#let s = \"one", "two", "three\""];
        let string: Vec<_> = pieces(&Grammar::Typst(Mode::Markup), &lines)
            .into_iter()
            .filter(|(_, _, _, class)| class == "string")
            .collect();
        assert_eq!(
            string,
            vec![
                (0, 9, "\"one".to_owned(), "string".to_owned()),
                (1, 0, "two".to_owned(), "string".to_owned()),
                (2, 0, "three\"".to_owned(), "string".to_owned()),
            ]
        );
    }
}
