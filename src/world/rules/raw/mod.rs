//! A replacement for typst-html's native `raw` show rule: mark a code block's
//! tokens with [`TOKEN`] and [`SCOPE`] instead of colouring them inline, so a
//! stylesheet owns the palette.

mod grammar;
mod scope;

use typst::ecow::EcoVec;
use typst::foundations::{Content, NativeElement, Packed, ShowFn};
use typst::layout::BlockElem;
use typst::text::{LinebreakElem, RawElem, RawLine, TextElem};
use typst_html::{HtmlAttr, HtmlAttrs, HtmlElem, tag};

use crate::config::Named;

use grammar::Grammar;

/// The info-string tags naming typst *markup*, which is what a fence body is.
///
/// Crate-visible so [`Grammar::of`] and the markdown lowerer's `Fence`, which
/// both ask whether a fence is typst, cannot answer differently.
pub const TYPST: &[&str] = &["typ", "typst"];

/// The mark naming the vocabulary entry a span's text resolved to, read and
/// removed by the transform that names it.
pub const TOKEN: HtmlAttr = HtmlAttr::constant("data-token");

/// The mark naming the grammar's own scope, kept in the output only when
/// `highlight { scopes }` asks for it.
pub const SCOPE: HtmlAttr = HtmlAttr::constant("data-scope");

/// The raw show rule: mark a code block's tokens instead of colouring them,
/// each line keeping the number, count and text a `show raw.line` rule expects.
pub(super) const RAW_RULE: ShowFn<RawElem> = |elem, engine, styles| {
    let lines = elem.lines.as_deref().unwrap_or_default();
    let grammar = Grammar::of(elem, engine, styles)?;

    let texts: Vec<_> = lines.iter().map(|line| line.text.clone()).collect();
    let mut bodies: Vec<Vec<Content>> = vec![Vec::new(); lines.len()];
    grammar.tokens(&texts, &mut |piece| {
        let span = lines[piece.line].span();
        let mut body = TextElem::packed(piece.text).spanned(span);
        if piece.offset > 0 {
            body = body.set(TextElem::span_offset, piece.offset);
        }
        if let Some((token, scope)) = piece.token {
            let mut attrs = HtmlAttrs::new();
            attrs.push(TOKEN, token.name());
            attrs.push(SCOPE, scope);
            body = HtmlElem::new(tag::span)
                .with_attrs(attrs)
                .with_body(Some(body))
                .pack()
                .spanned(span);
        }
        bodies[piece.line].push(body);
    });

    let mut seq = EcoVec::with_capacity((2 * lines.len()).saturating_sub(1));
    for (i, (line, body)) in lines.iter().zip(bodies).enumerate() {
        if i != 0 {
            seq.push(LinebreakElem::shared().clone());
        }
        seq.push(
            Packed::new(RawLine::new(
                line.number,
                line.count,
                line.text.clone(),
                Content::sequence(body),
            ))
            .spanned(line.span())
            .pack(),
        );
    }

    let lang = elem.lang.get_ref(styles);
    let code = HtmlElem::new(tag::code)
        .with_optional_attr(const { HtmlAttr::constant("data-lang") }, lang.clone())
        .with_body(Some(Content::sequence(seq)))
        .pack()
        .spanned(elem.span());

    Ok(if elem.block.get(styles) {
        BlockElem::packed(
            HtmlElem::new(tag::pre)
                .with_body(Some(code))
                .pack()
                .spanned(elem.span()),
        )
    } else {
        code
    })
};
