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

use typst::foundations::{NativeElement, ShowFn};
use typst::layout::BlockElem;
use typst::loading::DataSource;
use typst::visualize::ImageElem;
use typst_html::{HtmlAttrs, HtmlElem, attr, tag};

/// The `src` prefix marking an image the externalize pass must resolve. The
/// remainder is the source file's project-relative virtual path.
pub const MARKER: &str = "baudelaire:asset:";

/// The image show rule: emit a file-referencing marker instead of base64.
pub const IMAGE_RULE: ShowFn<ImageElem> = |elem, engine, styles| {
    let image = elem.decode(engine, styles)?;

    // The source's project-relative path, when it is a real file. Resolved the
    // same way typst itself resolves an image path (against the element's own
    // source file), so the marker names exactly the file typst read.
    let vpath = match &elem.source.source {
        DataSource::Path(path) => path
            .resolve_if_some(elem.span().id())
            .ok()
            .map(|id| id.intern())
            .map(|id| id.vpath().get_without_slash().to_owned()),
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

    Ok(BlockElem::packed(
        HtmlElem::new(tag::img)
            .with_attrs(attrs)
            .pack()
            .spanned(elem.span()),
    ))
};
