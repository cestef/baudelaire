//! `@baudelaire/html`'s `svg()` against a corpus of real-world SVG files.
//!
//! The fixtures in `tests/fixtures/svg` are the shapes an icon actually arrives
//! in: hand-written, exported from Inkscape or Illustrator, carrying gradients,
//! text, entities or an embedded raster. Each one is inlined into a page and the
//! resulting markup checked, because the failure mode this guards against is
//! silent: a dropped element leaves a smaller icon, not an error.

mod common;

use common::Site;

/// Build a site whose page inlines `fixture` with `attrs`, and return its HTML.
/// The fixture is copied in under `icons/`, so the corpus stays real files that
/// can be opened in a browser rather than strings in a test.
fn inline(fixture: &str, attrs: &str) -> String {
    let site = site(fixture, attrs);
    site.stats();
    site.output("index.html")
}

/// The same site, left unbuilt so a test can assert on the failure.
fn site(fixture: &str, attrs: &str) -> Site {
    let site = Site::with("site \"T\"\n");
    let body = std::fs::read_to_string(format!("tests/fixtures/svg/{fixture}"))
        .unwrap_or_else(|e| panic!("fixture {fixture}: {e}"));
    site.write(&format!("icons/{fixture}"), &body);
    site.write(
        "templates/page.typ",
        &format!(
            "#import \"@baudelaire/html:0.1.0\": svg\n\
             #let page(data, body) = svg(\"/icons/{fixture}\"{attrs})\n"
        ),
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHi.\n",
    );
    site
}

/// The failure message of a build that must not succeed.
fn refuses(fixture: &str) -> String {
    site(fixture, "").build_error().to_string()
}

/// A hand-written icon inlines verbatim: this is the shape the docs site ships,
/// and `currentColor` is the whole reason for inlining rather than `<img>`.
#[test]
fn a_hand_written_icon_round_trips() {
    let html = inline("lucide.svg", ", class: \"icon\"");
    assert!(html.contains(r#"<circle cx="11" cy="11" r="8">"#), "{html}");
    assert!(html.contains(r#"<path d="m21 21-4.3-4.3">"#), "{html}");
    assert!(html.contains(r#"stroke="currentColor""#), "{html}");
    assert!(html.contains(r#"class="icon""#), "{html}");
}

/// An Inkscape export keeps its drawing and loses the editor's bookkeeping.
///
/// The prefixes matter: roxmltree reports a name without one, so an unfiltered
/// `sodipodi:docname` would land as a plain `docname` and `dc:title` as an SVG
/// `<title>`, which is the icon's accessible name. A screen reader would then
/// announce "Untitled".
#[test]
fn an_inkscape_export_drops_editor_namespaces() {
    let html = inline("inkscape.svg", "");
    assert!(
        html.contains(r#"<path d="M1 1h22v22H1Z" id="p1">"#),
        "{html}"
    );
    // `xlink:href` is SVG 1.1's spelling of what SVG 2 calls `href`.
    assert!(html.contains(r##"<use href="#p1" x="2">"##), "{html}");
    for gone in [
        "sodipodi",
        "inkscape",
        "docname",
        "namedview",
        "zoom",
        "groupmode",
        "Untitled",
    ] {
        assert!(!html.contains(gone), "{gone} should be dropped: {html}");
    }
    // The XML declaration and the generator comment are not DOM either.
    assert!(!html.contains("<?xml"), "{html}");
    assert!(!html.contains("Inkscape"), "{html}");
}

/// An Illustrator export keeps its stylesheet intact.
///
/// `<style>` is raw text in HTML, so a `>` combinator must *not* be escaped or
/// the rule silently stops matching, and the CDATA wrapper must not survive.
#[test]
fn an_illustrator_export_keeps_its_stylesheet() {
    let html = inline("illustrator.svg", "");
    assert!(html.contains(".st0{fill:#231F20;}"), "{html}");
    assert!(html.contains(".st1 > .st2{stroke-width:0.5;}"), "{html}");
    assert!(
        !html.contains("&gt;"),
        "a css combinator must not be escaped: {html}"
    );
    assert!(!html.contains("CDATA"), "{html}");
    // The file's own DTD, not the page's `<!DOCTYPE html>`, which typst emits.
    assert!(!html.contains("DTD SVG"), "{html}");
    assert!(!html.contains("Adobe Illustrator"), "{html}");
    assert!(
        html.contains(r#"<polygon class="st0" points="1,1 23,1 12,23">"#),
        "{html}"
    );
}

/// Gradients, clip paths and filters: camelCase tags and attributes survive,
/// since typst only reserves *hyphenated* names.
#[test]
fn gradients_and_filters_keep_their_camel_case() {
    let html = inline("gradient.svg", "");
    assert!(
        html.contains(r#"<linearGradient id="g" gradientUnits="userSpaceOnUse""#),
        "{html}"
    );
    assert!(html.contains(r#"<clipPath id="c">"#), "{html}");
    assert!(
        html.contains(r#"<feGaussianBlur stdDeviation="2">"#),
        "{html}"
    );
    assert!(html.contains(r#"fill="url(#g)""#), "{html}");
    assert!(html.contains(r#"clip-path="url(#c)""#), "{html}");
    assert!(
        html.contains(r##"<stop offset="0" stop-color="#f00">"##),
        "{html}"
    );
}

/// Text content, entities and non-ASCII survive a parse-and-rebuild: entities
/// are resolved by the XML parser, then re-escaped only where HTML needs it.
#[test]
fn text_entities_and_unicode_survive() {
    let html = inline("text.svg", "");
    assert!(html.contains("A &amp; B © café"), "{html}");
    assert!(html.contains("Some &lt;description>"), "{html}");
    assert!(html.contains(r#"Café "quoted" &amp; more"#), "{html}");
    assert!(
        html.contains(r#"<tspan dx="2" font-weight="bold">bold</tspan>"#),
        "{html}"
    );
    assert!(
        html.contains("<title>"),
        "an accessible name is kept: {html}"
    );
}

/// The walk recurses to the bottom rather than stopping at a fixed depth.
#[test]
fn deeply_nested_groups_are_kept() {
    let html = inline("nested.svg", "");
    assert!(html.contains(r#"id="deep""#), "{html}");
    assert_eq!(
        html.matches("<g>").count(),
        5,
        "every level survives: {html}"
    );
}

/// A self-closing root is an icon with no children, not a failure.
#[test]
fn an_empty_root_inlines_as_an_empty_svg() {
    let html = inline("empty.svg", ", class: \"blank\"");
    assert!(html.contains(r#"viewBox="0 0 24 24""#), "{html}");
    assert!(html.contains(r#"class="blank""#), "{html}");
}

/// A file with no `xmlns` still inlines: an undeclared default namespace is
/// absent, not foreign, and refusing it would reject most icon sets.
#[test]
fn a_file_without_a_namespace_still_inlines() {
    let html = inline("bare.svg", "");
    assert!(html.contains(r#"<path d="M0 0h1">"#), "{html}");
    // The namespace is supplied, so the fragment stays valid if lifted out.
    assert!(
        html.contains(r#"xmlns="http://www.w3.org/2000/svg""#),
        "{html}"
    );
}

/// A `data:` URI passes through untouched: it holds `/`, `+` and `=`, which
/// nothing in attribute handling may rewrite.
#[test]
fn an_embedded_raster_survives() {
    let html = inline("raster.svg", "");
    assert!(
        html.contains("data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw=="),
        "{html}"
    );
}

/// A file that is not an SVG fails by name. Every child would be dropped for
/// being in a foreign namespace, so without this it would inline as an empty
/// `<svg>` and look like a styling problem.
#[test]
fn a_non_svg_root_is_refused() {
    let err = refuses("notsvg.svg");
    assert!(err.contains("not an SVG"), "{err}");
    assert!(err.contains("<config>"), "should name the root: {err}");
}

/// Malformed XML fails rather than shipping a partial icon.
#[test]
fn a_malformed_file_is_refused() {
    let err = refuses("malformed.svg");
    assert!(err.contains("well-formed"), "{err}");
}

/// Inlining puts the file inside the document, so anything active in it runs
/// with the page's origin. An icon set pulled from a package is exactly where
/// that would be a surprise, so it is a loud error, not a silent strip.
#[test]
fn active_content_is_refused() {
    for (fixture, what) in [
        ("script.svg", "`<script>`"),
        ("onload.svg", "`onload` handler"),
        ("javascript-href.svg", "`javascript:`"),
    ] {
        let err = site(fixture, "").build_error().to_string();
        assert!(err.contains("would run on every page"), "{fixture}: {err}");
        assert!(err.contains(what), "{fixture} should name it: {err}");
    }
}

/// An SVG 1.1 tag typst's HTML writer reserves, nested where a lazy check would
/// miss it.
#[test]
fn a_reserved_tag_is_refused_by_name() {
    let err = refuses("font-face.svg");
    assert!(err.contains("font-face"), "{err}");
    assert!(err.contains("reserved"), "{err}");
}

/// Every fixture that should build, does: a guard against a future change that
/// makes one of them fail for a reason no single test above would notice.
#[test]
fn every_valid_fixture_builds() {
    for fixture in [
        "lucide.svg",
        "inkscape.svg",
        "illustrator.svg",
        "gradient.svg",
        "text.svg",
        "nested.svg",
        "empty.svg",
        "bare.svg",
        "raster.svg",
    ] {
        let html = inline(fixture, "");
        assert!(html.contains("<svg"), "{fixture} produced no svg: {html}");
        assert!(
            !html.contains("data-baudelaire-svg"),
            "{fixture} leaked the marker: {html}"
        );
    }
}

/// A file carrying the transform's own marker must not be re-read.
///
/// The marker is stripped from the caller's element before the file is spliced
/// in, but the walk continues into the nodes just placed. Without skipping the
/// marker while copying the file's attributes, a self-referencing icon recursed
/// until the stack gave out and the process aborted with no diagnostic at all.
#[test]
fn a_marker_inside_the_file_does_not_recurse() {
    let html = inline("marker-child.svg", "");
    assert!(html.contains(r#"<circle r="8">"#), "{html}");
    assert!(!html.contains("data-baudelaire-svg"), "{html}");
}

/// The same on the root element, where the failure is silent rather than loud:
/// the walk has already passed that element, so the marker is merged into the
/// attributes and shipped.
#[test]
fn a_marker_on_the_files_root_does_not_leak() {
    let html = inline("marker-root.svg", ", class: \"icon\"");
    assert!(html.contains(r#"<circle r="8">"#), "{html}");
    assert!(html.contains(r#"class="icon""#), "{html}");
    assert!(!html.contains("data-baudelaire-svg"), "{html}");
}

/// A browser strips tabs and newlines out of a URL before deciding its scheme,
/// so `java&#9;script:` navigates as `javascript:`. Comparing the raw text would
/// pass exactly the spelling someone hiding a payload would use.
#[test]
fn obfuscated_javascript_urls_are_refused() {
    for fixture in ["js-whitespace.svg", "js-xlink.svg"] {
        let err = refuses(fixture);
        assert!(err.contains("would run on every page"), "{fixture}: {err}");
    }
}

/// An inlined `<style>` is an ordinary page stylesheet, so an Illustrator
/// export's `.st0` would repaint every `.st0` on the page. Each rule is confined
/// to the icon that carries it.
#[test]
fn a_stylesheet_is_confined_to_its_icon() {
    let html = inline("illustrator.svg", "");
    assert!(html.contains("data-svg="), "the icon is marked: {html}");
    // Zero specificity, so confining a rule does not also let it outrank the
    // page's own CSS.
    assert!(
        html.contains(r#":where([data-svg="#),
        "rules are confined: {html}"
    );
    assert!(
        !html.contains("\n\t.st0{"),
        "no rule escapes unconfined: {html}"
    );
}

/// An icon with no stylesheet gains no scope attribute: the marking exists only
/// to give the confined rules something to match against.
#[test]
fn an_icon_without_styles_is_not_marked() {
    let html = inline("lucide.svg", "");
    assert!(!html.contains("data-svg="), "{html}");
}

/// `@keyframes` names an animation rather than selecting elements, so confining
/// its `from`/`to` would break the animation instead of scoping it. `@media`
/// does hold style rules, and they are confined.
#[test]
fn at_rules_are_confined_only_where_they_hold_selectors() {
    let html = inline("animated.svg", "");
    assert!(
        html.contains("@keyframes spin { from { transform: rotate(0) }"),
        "keyframes are untouched: {html}"
    );
    assert!(
        html.contains(r#"@media (prefers-reduced-motion: reduce) { :where([data-svg="#),
        "rules inside @media are confined: {html}"
    );
    assert!(
        html.contains("animation: spin 1s linear infinite"),
        "{html}"
    );
}
