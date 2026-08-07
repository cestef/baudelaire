//! A replacement for typst-html's native image show rule.
//!
//! typst's built-in rule inlines every `image()` into the DOM as a base64
//! `data:` URI: no file, no name, and the page carries the image bytes inline.
//! This rule instead emits `<img src="baudelaire:asset:<vpath>">`, a marker
//! carrying the source file's project-relative path. A later baudelaire pass
//! (`render::transform::externalize`) resolves the marker: it copies the file under the
//! asset URL and rewrites `src` to the served path. Images with no source file
//! (`image(bytes: ..)`, or a re-encoded pixel format) keep the base64 URI, since
//! there is nothing on disk to reference.
//!
//! The rule is installed by [`super::Rules`] only when `images.extract` is on. It is a bare `fn` with no captured state, so it can
//! only emit markup; all IO, naming, and caching stay in baudelaire's pass.
//!
//! Fidelity note: typst-html's own `css` module is private, so the sizing CSS
//! the native rule reserves for `image(width: ..)` is reproduced here rather
//! than reused (see [`Css`]). An image sized in typst keeps that size under
//! `extract`, and the reproduction is checked by
//! `tests/scenarios/images.kdl`, which renders one sized image under both
//! settings of `images.extract` and asserts the same `style`.

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
    // Pixel dimensions of a decoded raster, so far inside `i64` that no image
    // typst can hold could truncate; kept as typst's own rule spells it.
    #[allow(clippy::cast_possible_truncation)]
    let cast = |v: f64| format!("{}", v.round() as i64);
    attrs.push(attr::width, cast(image.width()));
    attrs.push(attr::height, cast(image.height()));

    // Reproduce the sizing typst's native rule sets as inline CSS: pixel-hinting,
    // and the author's `width`/`height`. typst-html's own `css` builder is
    // private, so the same values are rendered here (see [`Css`]).
    let mut props: Vec<(&str, String)> = Vec::new();
    if let Some(rendering) = typst_svg::convert_image_scaling(image.scaling()) {
        props.push(("image-rendering", rendering.to_owned()));
    }
    if let Smart::Custom(width) = elem.width.get(styles) {
        props.push(("width", Css(&width).to_string()));
    }
    if let Sizing::Rel(height) = elem.height.get(styles) {
        props.push(("height", Css(&height).to_string()));
    }
    // Down to the whitespace and the ordering: typst's `css::Properties` keeps
    // its entries sorted by property name and writes them `name: value`, joined
    // by `; ` with none trailing. A page built with `extract` off is typst's own
    // output, so anything this rule spells differently is a diff between two
    // builds of the same page.
    props.sort_by_key(|(name, _)| *name);
    let mut style = String::new();
    for (name, value) in props {
        if !style.is_empty() {
            style.push_str("; ");
        }
        let _ = write!(style, "{name}: {value}");
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

/// Displays a relative length as a CSS dimension, reproducing typst-html's own
/// (private) `ToCss` encoding term for term: a ratio becomes a percent, an
/// em-length ems, an absolute length points, and a mix becomes a `calc(..)`
/// sum. A zero term is dropped, a single term is emitted bare, and an all-zero
/// length is `0`.
///
/// Every detail below is upstream's, not a choice made here, because the whole
/// value of this type is that a page renders the same under both settings of
/// `images.extract`. Two of them were wrong and produced valid CSS anyway,
/// which is why they survived: a negative term was summed rather than
/// subtracted (`50% - 10pt` came out `calc(50% + -10pt)`), and every term was
/// rounded to four decimals where upstream rounds a ratio to two (`100%/3` came
/// out `33.3333%` against upstream's `33.33%`).
struct Css<'a>(&'a Rel<Length>);

impl Css<'_> {
    /// Decimal places kept, per term, as typst-html keeps them: a ratio to two,
    /// a length of either kind to four.
    const RATIO: i16 = 2;
    const LENGTH: i16 = 4;
}

impl std::fmt::Display for Css<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rel = self.0;
        // The three terms typst sums, in the order it sums them.
        let terms = [
            (rel.rel.get() * 100.0, Self::RATIO, "%"),
            (rel.abs.em.get(), Self::LENGTH, "em"),
            (rel.abs.abs.to_pt(), Self::LENGTH, "pt"),
        ];
        let mut sum = String::new();
        let mut written = 0;
        for (value, precision, unit) in terms {
            // Zero is tested before rounding, as upstream tests it: a term too
            // small to survive its own precision still counts as a term, and
            // prints as the `0` it rounded to.
            if value == 0.0 {
                continue;
            }
            let round = |v: f64| typst::utils::round_with_precision(v, precision);
            match written {
                0 => write!(sum, "{}{unit}", round(value))?,
                // Negated and subtracted rather than summed, so a negative term
                // reads as the CSS an author would have written.
                _ if value < 0.0 => write!(sum, " - {}{unit}", round(-value))?,
                _ => write!(sum, " + {}{unit}", round(value))?,
            }
            written += 1;
        }
        match written {
            0 => f.write_str("0"),
            1 => f.write_str(&sum),
            _ => write!(f, "calc({sum})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Css;
    use typst::layout::{Abs, Em, Length, Ratio, Rel};

    fn css(rel: &Rel<Length>) -> String {
        Css(rel).to_string()
    }

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

    #[test]
    fn css_subtracts_a_negative_term_rather_than_summing_it() {
        let rel = Rel::new(Ratio::new(0.5), Length::from(Abs::pt(-10.0)));
        assert_eq!(css(&rel), "calc(50% - 10pt)");
    }

    #[test]
    fn css_emits_a_lone_negative_term_as_written() {
        let rel = Rel::new(Ratio::zero(), Length::from(Abs::pt(-10.0)));
        assert_eq!(css(&rel), "-10pt");
    }

    #[test]
    fn css_rounds_a_ratio_to_two_decimals_and_a_length_to_four() {
        let third = Rel::new(Ratio::new(1.0 / 3.0), Length::zero());
        assert_eq!(css(&third), "33.33%");
        let em = Rel::new(Ratio::zero(), Length::from(Em::new(1.0 / 3.0)));
        assert_eq!(css(&em), "0.3333em");
    }

    #[test]
    fn css_keeps_a_term_too_small_for_its_own_precision() {
        let rel = Rel::new(Ratio::new(0.5), Length::from(Abs::pt(0.000_01)));
        assert_eq!(css(&rel), "calc(50% + 0pt)");
    }
}
