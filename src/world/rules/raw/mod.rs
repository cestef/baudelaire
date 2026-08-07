//! A replacement for typst-html's native `raw` show rule.
//!
//! typst's built-in rule bakes a highlight theme's palette into the markup, one
//! `style="color: #rrggbb"` per token. A colour frozen at build time cannot
//! follow a light/dark toggle that flips at runtime, so a themed site used to
//! author a `.tmTheme` whose colours were *meaningless sentinels* and win them
//! back with `pre code [style*="e5d004"] { color: var(--kw) !important }`: an
//! attribute-substring selector fighting an inline style. This rule emits a span
//! per token instead, and leaves the palette to the stylesheet where a toggle
//! can reach it.
//!
//! What it emits is a *mark*, not a class: `data-token` naming the vocabulary
//! entry the grammar's scopes resolved to ([`scope`]), and `data-scope` naming
//! the scope itself. What those become on the page is the site's to configure
//! (`html { highlight { } }`), and a show rule is a bare `fn` with nowhere to
//! keep a config, so the naming is one DOM pass later in
//! `render::transform::highlight`. Nothing marked survives into the output: a
//! token the site kept is a class, and one it dropped is spliced away.
//!
//! Where the marks come from, since it is not from the rule's own element:
//! typst highlights during *synthesis*, before any show rule runs, and reduces a
//! scope to a colour on the way (`Packed<RawElem>::highlight`). By here the
//! scopes are gone, so the block is highlighted again from scratch. What is
//! reused is `elem.lines`, whose `text` is each line as typst preprocessed it
//! (tabs expanded, breaks split), so the one part of that pipeline this rule
//! must not disagree with is the one part it does not reproduce. Everything else
//! is [`grammar`].
//!
//! Installed by [`super::Rules`] only when `html { highlight }` is on. Two
//! things typst's rule does that this deliberately drops: a theme's `font-style`
//! and `font-weight` per scope (CSS's to own now), and the highlighting work
//! synthesis did anyway, which is thrown away.

mod grammar;
mod scope;

use typst::ecow::EcoVec;
use typst::foundations::{Content, NativeElement, Packed, ShowFn};
use typst::layout::BlockElem;
use typst::text::{LinebreakElem, RawElem, RawLine, TextElem};
use typst_html::{HtmlAttr, HtmlAttrs, HtmlElem, tag};

use crate::config::Named;

use grammar::Grammar;

/// The mark naming the vocabulary entry a span's text resolved to, read and
/// removed by the transform that names it. Written here, so the two halves of
/// the pipeline cannot drift apart on the attribute they meet at.
pub const TOKEN: HtmlAttr = HtmlAttr::constant("data-token");

/// The mark naming the grammar's own scope, kept in the output only when
/// `highlight { scopes }` asks for it.
pub const SCOPE: HtmlAttr = HtmlAttr::constant("data-scope");

/// The raw show rule: mark a code block's tokens instead of colouring them.
pub(super) const RAW_RULE: ShowFn<RawElem> = |elem, engine, styles| {
    let lines = elem.lines.as_deref().unwrap_or_default();
    let grammar = Grammar::of(elem, engine, styles)?;

    // The lines as typst preprocessed them, which is all the highlighter needs.
    let texts: Vec<_> = lines.iter().map(|line| line.text.clone()).collect();
    let mut bodies: Vec<Vec<Content>> = vec![Vec::new(); lines.len()];
    grammar.tokens(&texts, &mut |piece| {
        // The line's own span, offset to where the piece starts inside it: what
        // typst's own highlighting hands a diagnostic pointing into a block.
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

    // A line keeps its number, its count and its plain text, so a `show
    // raw.line` rule sees what it always did; only the body is this rule's.
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

    // The wrapper, as typst's own rule builds it.
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
