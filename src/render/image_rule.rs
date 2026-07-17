//! A replacement for typst-html's native image show rule.
//!
//! typst's built-in rule inlines every `image()` into the DOM as a base64
//! `data:` URI: no file, no name, and the page carries the image bytes inline.
//! This rule instead emits `<img src="baudelaire:asset:<vpath>">`, a marker
//! carrying the source file's project-relative path. A later baudelaire pass
//! ([`super::externalize`]) resolves the marker: it copies the file under the
//! asset URL and rewrites `src` to the served path. Images with no source file
//! (`image(bytes: ..)`, or a re-encoded pixel format) keep the base64 URI, since
//! there is nothing on disk to reference.
//!
//! The rule is installed on the built [`typst::Library`] via
//! `rules.replace(Target::Html, IMAGE_RULE)` (see [`crate::world`]) only when
//! `images.extract` is on. It is a bare `fn` with no captured state, so it can
//! only emit markup; all IO, naming, and caching stay in baudelaire's pass.
//!
//! Fidelity note: typst-html's own `css` module (which the native rule uses to
//! reserve layout for `image(width: ..)` sizing) is private, so this rule
//! reproduces only the pixel `width`/`height` attributes, not the sizing CSS.
//! Explicit typst image sizing is therefore not preserved under `extract`; size
//! with CSS instead.

use std::fmt::Write;

use typst::foundations::{NativeElement, ShowFn, Smart};
use typst::layout::{BlockElem, Length, Rel, Sizing};
use typst::loading::DataSource;
use typst::syntax::VirtualRoot;
use typst::visualize::ImageElem;
use typst_html::{HtmlAttrs, HtmlElem, attr, tag};

/// The `src` prefix marking an image the externalize pass must resolve. The
/// remainder is the source file's project-relative virtual path.
pub const MARKER: &str = "baudelaire:asset:";

/// The image show rule: emit a file-referencing marker instead of base64.
pub const IMAGE_RULE: ShowFn<ImageElem> = |elem, engine, styles| {
    let image = elem.decode(engine, styles)?;

    // The source's project-relative path, when it is a real project file.
    // Resolved the same way typst itself resolves an image path (against the
    // element's own source file), so the marker names exactly the file typst
    // read. A package image (`@preview/..`) lives outside the project root the
    // externalize pass copies from, so it is left inline rather than mis-copied.
    let vpath = match &elem.source.source {
        DataSource::Path(path) => path
            .resolve_if_some(elem.span().id())
            .ok()
            .filter(|rooted| matches!(rooted.root(), VirtualRoot::Project))
            .map(|rooted| rooted.vpath().get_without_slash().to_owned()),
        DataSource::Bytes(_) => None,
    };

    let mut attrs = HtmlAttrs::new();
    match vpath {
        // A real file: mark it for externalization.
        Some(vpath) => attrs.push(attr::src, format!("{MARKER}{vpath}")),
        // No file to reference: keep typst's inline data URI.
        None => attrs.push(attr::src, typst_svg::WebImage::new(&image).to_base64_url()),
    }

    if let Some(alt) = elem.alt.get_cloned(styles) {
        attrs.push(attr::alt, alt);
    }
    // Intrinsic pixel dimensions, so the browser can reserve space before the
    // file loads (rounded, matching typst's own rule).
    let cast = |v: f64| format!("{}", v.round() as i64);
    attrs.push(attr::width, cast(image.width()));
    attrs.push(attr::height, cast(image.height()));

    // Reproduce the sizing typst's native rule sets as inline CSS: pixel-hinting,
    // and the author's `width`/`height`. typst-html's own `css` builder is
    // private, so the same values are rendered here (see [`css`]).
    let mut style = String::new();
    if let Some(rendering) = typst_svg::convert_image_scaling(image.scaling()) {
        let _ = write!(style, "image-rendering:{rendering};");
    }
    if let Smart::Custom(width) = elem.width.get(styles) {
        let _ = write!(style, "width:{};", css(&width));
    }
    if let Sizing::Rel(height) = elem.height.get(styles) {
        let _ = write!(style, "height:{};", css(&height));
    }
    if !style.is_empty() {
        attrs.push(attr::style, style);
    }

    Ok(BlockElem::packed(
        HtmlElem::new(tag::img)
            .with_attrs(attrs)
            .pack()
            .spanned(elem.span()),
    ))
};

/// A relative length as a CSS dimension, mirroring typst-html's own (private)
/// `ToCss` encoding: a ratio becomes a percent, an absolute length points, an
/// em-length ems, and a mix becomes a `calc(..)` sum. A single term is emitted
/// bare, and an all-zero length is `0`.
fn css(rel: &Rel<Length>) -> String {
    let mut terms = Vec::new();
    if rel.rel.get() != 0.0 {
        terms.push(format!("{}%", trim(rel.rel.get() * 100.0)));
    }
    if rel.abs.em.get() != 0.0 {
        terms.push(format!("{}em", trim(rel.abs.em.get())));
    }
    if rel.abs.abs.to_pt() != 0.0 {
        terms.push(format!("{}pt", trim(rel.abs.abs.to_pt())));
    }
    match terms.len() {
        0 => "0".into(),
        1 => terms.pop().unwrap(),
        _ => format!("calc({})", terms.join(" + ")),
    }
}

/// A finite number formatted for CSS: up to four decimals, with trailing zeros
/// (and any bare decimal point) trimmed, so `50.0` renders as `50`.
fn trim(value: f64) -> String {
    let s = format!("{value:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_owned()
}

#[cfg(test)]
mod tests {
    use super::css;
    use typst::layout::{Abs, Em, Length, Ratio, Rel};

    #[test]
    fn css_renders_a_ratio_as_a_percent() {
        let rel = Rel::new(Ratio::new(0.5), Length::zero());
        assert_eq!(css(&rel), "50%");
    }

    #[test]
    fn css_renders_absolute_and_em_lengths() {
        let pt = Rel::new(Ratio::zero(), Length::from(Abs::pt(200.0)));
        assert_eq!(css(&pt), "200pt");
        let em = Rel::new(Ratio::zero(), Length::from(Em::new(1.5)));
        assert_eq!(css(&em), "1.5em");
    }

    #[test]
    fn css_sums_mixed_terms_into_a_calc() {
        let rel = Rel::new(Ratio::new(0.5), Length::from(Abs::pt(10.0)));
        assert_eq!(css(&rel), "calc(50% + 10pt)");
    }

    #[test]
    fn css_of_zero_is_zero() {
        assert_eq!(css(&Rel::new(Ratio::zero(), Length::zero())), "0");
    }
}
