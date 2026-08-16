//! A replacement for typst-html's native image show rule: emit a marker naming
//! the source file instead of inlining the image as a base64 `data:` URI.

use std::fmt::Write;

use typst::foundations::{NativeElement, ShowFn, Smart};
use typst::layout::{BlockElem, Length, Rel, Sizing};
use typst::loading::DataSource;
use typst::syntax::VirtualRoot;
use typst::visualize::ImageElem;
use typst_html::{HtmlAttrs, HtmlElem, attr, tag};

/// The `src` prefix marking an image the externalize pass must resolve,
/// followed by the source file's project-relative virtual path.
pub const MARKER: &str = "baudelaire:asset:";

/// The image show rule: emit a file-referencing marker instead of base64, with
/// the sizing typst's own rule would emit reproduced term for term, down to the
/// property order.
pub const IMAGE_RULE: ShowFn<ImageElem> = |elem, engine, styles| {
    let image = elem.decode(engine, styles)?;

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
        Some(vpath) => attrs.push(attr::src, format!("{MARKER}{vpath}")),
        None => attrs.push(attr::src, typst_svg::WebImage::new(&image).to_base64_url()),
    }

    if let Some(alt) = elem.alt.get_cloned(styles) {
        attrs.push(attr::alt, alt);
    }
    // Pixel dimensions of a decoded raster, far inside `i64` either way.
    #[allow(clippy::cast_possible_truncation)]
    let cast = |v: f64| format!("{}", v.round() as i64);
    attrs.push(attr::width, cast(image.width()));
    attrs.push(attr::height, cast(image.height()));

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

/// Displays a relative length as a CSS dimension: a ratio becomes a percent, an
/// em-length ems, an absolute length points, and a mix a `calc(..)` sum.
///
/// A reimplementation of typst-html's private `ToCss`, pinned to typst 0.15,
/// with no upstream contract behind it.
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
        let terms = [
            (rel.rel.get() * 100.0, Self::RATIO, "%"),
            (rel.abs.em.get(), Self::LENGTH, "em"),
            (rel.abs.abs.to_pt(), Self::LENGTH, "pt"),
        ];
        let mut sum = String::new();
        let mut written = 0;
        for (value, precision, unit) in terms {
            if value == 0.0 {
                continue;
            }
            let round = |v: f64| typst::utils::round_with_precision(v, precision);
            match written {
                0 => write!(sum, "{}{unit}", round(value))?,
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
