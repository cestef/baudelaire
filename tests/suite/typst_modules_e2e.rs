//! The `@baudelaire/*` virtual Typst modules: a template imports one and the
//! compiler resolves it from memory, with nothing on disk.

use crate::common::Site;

/// A failed build's diagnostics as one string, the message that matters being
/// on the nested per-source diagnostic.
fn diagnostics(site: &Site) -> String {
    format!("{:?}", site.build_error())
}

/// A failed build's top-level message; only the typst-compile variant hides
/// its text in a nested field.
fn message(site: &Site) -> String {
    site.build_error().to_string()
}

/// A site whose single template is `body`, wrapped so each test writes only the
/// markup it cares about.
fn site(template: &str) -> Site {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\nauthor \"cstef\"\n");
    site.write(
        "templates/page.typ",
        &format!("#let page(data, body) = {{\n{template}\n}}\n"),
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHi.\n",
    );
    site
}

/// Named arguments become attributes, positional ones children, and a
/// hyphenated name needs no quoting.
#[test]
fn named_arguments_become_attributes() {
    let site = site(
        r#"
        import "@baudelaire/html:0.1.0": h
        h("button", class: "icon-btn", type: "button", aria-label: "Go")[x]
        "#,
    );
    site.stats();

    let html = site.output("index.html");
    assert!(
        html.contains(r#"<button class="icon-btn" type="button" aria-label="Go">x</button>"#),
        "{html}"
    );
}

/// Booleans and absent values resolve the way HTML wants: `true` writes a bare
/// attribute, `false` and `none` drop it, and anything else is coerced.
#[test]
fn values_coerce_and_absent_attributes_disappear() {
    let site = site(
        r#"
        import "@baudelaire/html:0.1.0": h
        h("input", type: "checkbox", checked: true, disabled: false, name: none, tabindex: 3)
        "#,
    );
    site.stats();

    let html = site.output("index.html");
    assert!(
        html.contains(r#"<input type="checkbox" checked tabindex="3">"#),
        "{html}"
    );
    // Scoped to the element: the generated `<head>` carries `name=` of its own.
    assert!(!html.contains("disabled"), "{html}");
}

/// `classes` drops what is absent and honours a `(name, condition)` pair, where
/// `"a" + if cond { " b" }` would yield `none` on the else branch and fail.
#[test]
fn classes_joins_conditionally() {
    let site = site(
        r#"
        import "@baudelaire/html:0.1.0": h, classes
        h("p", class: classes("callout", "callout-" + "note", ("on", true), ("off", false)))[x]
        h("p", class: classes(("off", false)))[y]
        "#,
    );
    site.stats();

    let html = site.output("index.html");
    assert!(
        html.contains(r#"<p class="callout callout-note on">x</p>"#),
        "{html}"
    );
    // An empty join is `none`, which `h` then omits: no `class=""`.
    assert!(html.contains("<p>y</p>"), "{html}");
}

/// `@baudelaire/site` binds site identity, and every key exists even when the
/// config leaves it unset, so a theme can read `author` off an authorless site.
#[test]
fn site_identity_is_bound() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    site.write(
        "templates/page.typ",
        r#"
        #import "@baudelaire/site:0.1.0": title, url, author, lang, languages
        #let page(data, body) = [#title|#url|#lang|#repr(author)|#languages.len()]
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHi.\n",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("T|https://example.com|en|none|0"), "{html}");
}

/// `feeds` names the files the feed pass actually wrote, per language and under
/// the configured name, so a template linking one cannot point at a file that
/// does not exist.
#[test]
fn site_feeds_name_what_the_build_writes() {
    let site = Site::with(
        "site \"T\"\nurl \"https://example.com\"\nlang \"en\"\n\
         languages {\n  en { name \"English\" }\n  fr { name \"Français\" }\n}\n\
         generate {\n  feed {\n    formats \"rss\" \"atom\"\n    names { rss \"index.xml\" }\n  }\n}\n",
    );
    site.write(
        "templates/page.typ",
        r#"
        #import "@baudelaire/site:0.1.0": feed-url, feeds
        #let page(data, body) = [
          #for feed in feeds [#feed.format=#feed-url(feed, data.lang);]
        ]
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHi.\n",
    );
    site.write(
        "content/index.fr.typ",
        "#let frontmatter = (title: \"Accueil\", template: \"page.typ\",)\nSalut.\n",
    );
    site.stats();

    let english = site.output("index.html");
    assert!(
        english.contains("rss=/index.xml") && english.contains("atom=/atom.xml"),
        "the configured name, at the root: {english}"
    );
    let french = site.output("fr/index.html");
    assert!(
        french.contains("rss=/fr/index.xml"),
        "the feed of this page's own language: {french}"
    );
    assert!(site.exists("public/fr/index.xml"), "and it was written");
}

/// A build that writes no feed binds an empty list, so a footer drawing them
/// draws nothing rather than a dead link.
#[test]
fn site_feeds_are_empty_without_a_feed() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    site.write(
        "templates/page.typ",
        r#"
        #import "@baudelaire/site:0.1.0": feeds
        #let page(data, body) = [count=#feeds.len()]
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHi.\n",
    );
    site.stats();

    assert!(site.output("index.html").contains("count=0"));
}

/// `@baudelaire/pages` hands a template the site's own catalogue, in the same
/// row shape a generated listing's `entries` carry, listings excluded.
#[test]
fn the_page_catalogue_is_bound_per_language() {
    let site = Site::with(
        "site \"T\"\nurl \"https://example.com\"\n\
         content {\n  taxonomies {\n    tags\n  }\n  collections {\n    posts { template \"page.typ\" }\n  }\n}\n",
    );
    site.write(
        "templates/page.typ",
        r#"
        #import "@baudelaire/pages:0.1.0": pages
        #let page(data, body) = [
          #for entry in pages("en") [
            #entry.collection/#entry.label/#entry.date/#entry.extra.at("hero", default: "-")/#entry.taxonomies.at("tags", default: ()).len()/#(if entry.description == none { "-" } else { entry.description })/#entry.extra.at("summary", default: "gone");
          ]
        ]
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHi.\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (\n  title: \"Hello\",\n  date: datetime(year: 2026, month: 7, day: 14),\n  tags: (\"rust\",),\n  summary: \"A summary.\",\n  hero: \"cover.png\",\n)\nx\n",
    );
    site.stats();

    let html = site.output("index.html");
    // An undated page carries `date: none`, which prints as nothing.
    assert!(
        html.contains("_root/Home//-/0/-/gone"),
        "root page row: {html}"
    );
    // `description` resolves from the `summary` alias, and a declared key is
    // *not* also in `extra`.
    assert!(
        html.contains("posts/Hello/2026-07-14/cover.png/1/A summary./gone"),
        "post row carries date, extra, taxonomies and the resolved description: {html}"
    );
    assert!(
        !html.contains("/Tags/"),
        "generated listings excluded: {html}"
    );
}

/// Discovery evaluates a page whole to reach its `frontmatter`, so a content
/// page's import lands before the build has written the table it asks for; the
/// read is answered empty and re-read once the real table is on disk.
#[test]
fn a_content_page_reads_the_catalogue_on_a_first_build() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    site.write(
        "content/index.typ",
        r#"
        #let frontmatter = (title: "Home",)
        #import "@baudelaire/site:0.1.0": lang
        #import "@baudelaire/pages:0.1.0": pages
        Pages: #pages(lang).len()
        "#,
    );
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nx\n");
    site.write("content/b.typ", "#let frontmatter = (title: \"B\",)\nx\n");
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("Pages: 3"), "{html}");
}

/// ...and adding a page invalidates the pages that read the catalogue, in the
/// build that added it.
#[test]
fn the_catalogue_a_content_page_reads_tracks_the_page_set() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    site.write(
        "content/index.typ",
        r#"
        #let frontmatter = (title: "Home",)
        #import "@baudelaire/pages:0.1.0": pages
        Pages: #pages("en").len()
        "#,
    );
    site.stats();
    assert!(site.output("index.html").contains("Pages: 1"));

    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nx\n");
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("Pages: 2"), "{html}");
}

/// ...and a page whose *frontmatter* reads the catalogue tracks it too.
///
/// The table is read during discovery, before the build has written it, so a
/// cold build legitimately sees the empty one and the discovery cache has to
/// record a read of a file that does not exist yet.
#[test]
fn frontmatter_derived_from_the_catalogue_re_evaluates_once_it_exists() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    site.write(
        "content/index.typ",
        r#"
        #import "@baudelaire/pages:0.1.0": pages
        #let frontmatter = (title: "Home: " + str(pages("en").len()),)
        body
        "#,
    );
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nx\n");
    site.stats();

    // Nothing was written when discovery ran, so the empty table is honest.
    assert!(
        site.output("index.html").contains("Home: 0"),
        "{}",
        site.output("index.html")
    );

    site.stats();
    let html = site.output("index.html");
    assert!(html.contains("Home: 2"), "{html}");
    assert!(!html.contains("Home: 0"), "{html}");
}

#[test]
fn an_unknown_module_suggests_the_nearest() {
    let site = site("import \"@baudelaire/htlm:0.1.0\": h\n[x]");
    let err = diagnostics(&site);
    assert!(err.contains("unknown baudelaire module `htlm`"), "{err}");
    // The suggestion and the list travel through typst, which renders no
    // baudelaire markup, so both arrive plain and on one line.
    assert!(err.contains("did you mean html?"), "{err}");
    // The list names what the running flavor actually serves.
    let valid = if cfg!(feature = "markdown") {
        "valid modules: html, markdown, pages, sections, site"
    } else {
        "valid modules: html, pages, sections, site"
    };
    assert!(err.contains(valid), "{err}");
}

/// A version the registry does not serve fails at the import instead of
/// reaching for the network, and answers with the line to write.
#[test]
fn an_unserved_version_is_rejected() {
    let site = site("import \"@baudelaire/html:9.9.9\": h\n[x]");
    let err = diagnostics(&site);
    assert!(err.contains("@baudelaire/html:0.1.0"), "{err}");
    // The natural wrong guess is baudelaire's own version.
    assert!(err.contains("not baudelaire's own version"), "{err}");
}

/// A page importing a virtual module still caches: the module resolves to no
/// file, and a dependency that cannot be hashed must not read as one that did.
#[test]
fn a_page_importing_a_module_still_caches() {
    let site = site("import \"@baudelaire/html:0.1.0\": h\nh(\"p\")[x]");
    assert_eq!(site.stats().cached, 0, "first build compiles");
    assert_eq!(site.stats().cached, 1, "second build reuses");
}

/// An icon file, and a template that inlines it with `svg()`.
fn icons(icon: &str, call: &str) -> Site {
    let site = site(&format!("import \"@baudelaire/html:0.1.0\": svg\n{call}"));
    site.write("icons/i.svg", icon);
    site
}

/// The file's own nodes land in the page as real DOM, which is what lets
/// `stroke=\"currentColor\"` resolve and a theme toggle recolour it.
#[test]
fn an_svg_file_is_inlined_as_dom() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" stroke=\"currentColor\">\n\
         <!-- dropped -->\n<circle cx=\"11\" cy=\"11\" r=\"8\"/>\n<path d=\"m21 21-4.3-4.3\"/>\n</svg>\n",
        "svg(\"/icons/i.svg\", class: \"icon\")",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains(r#"<circle cx="11" cy="11" r="8">"#), "{html}");
    assert!(html.contains(r#"<path d="m21 21-4.3-4.3">"#), "{html}");
    assert!(html.contains(r#"stroke="currentColor""#), "{html}");
    assert!(html.contains(r#"class="icon""#), "{html}");
    // The marker is transient, and XML comments are not DOM.
    assert!(!html.contains("data-baudelaire-svg"), "{html}");
    assert!(!html.contains("dropped"), "{html}");
}

/// The caller's attributes win over the file's, so one file serves every size
/// without a variant per call site.
#[test]
fn caller_attributes_override_the_files() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" fill=\"none\"/>\n",
        "svg(\"/icons/i.svg\", width: 16, height: 16)",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains(r#"width="16" height="16""#), "{html}");
    assert!(!html.contains(r#"width="24""#), "{html}");
    assert!(html.contains(r#"fill="none""#), "{html}");
}

/// A camelCase SVG tag is fine: typst only reserves *hyphenated* names, so
/// gradients and filters inline like anything else.
#[test]
fn camel_case_svg_tags_inline() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n\
         <linearGradient id=\"g\"><stop offset=\"0\"/></linearGradient>\n\
         <filter><feGaussianBlur stdDeviation=\"2\"/></filter>\n</svg>\n",
        "svg(\"/icons/i.svg\")",
    );
    site.stats();

    let html = site.output("index.html");
    // The id is scoped to the file it came from, so what this pins is the tag.
    assert!(html.contains(r#"<linearGradient id="g-"#), "{html}");
    assert!(
        html.contains(r#"<feGaussianBlur stdDeviation="2">"#),
        "{html}"
    );
}

/// The SVG 1.1 tags typst's HTML writer refuses, each named by the failure
/// rather than silently dropped.
#[test]
fn reserved_svg_tags_fail_by_name() {
    for tag in [
        "font-face",
        "font-face-src",
        "font-face-uri",
        "font-face-format",
        "font-face-name",
        "missing-glyph",
        "color-profile",
        "annotation-xml",
    ] {
        let site = icons(
            &format!("<svg xmlns=\"http://www.w3.org/2000/svg\"><{tag}/></svg>\n"),
            "svg(\"/icons/i.svg\")",
        );
        let err = message(&site);
        assert!(err.contains(tag), "should name the tag: {err}");
        assert!(err.contains("reserved"), "should say why: {err}");
    }
}

/// To typst a hyphen means a custom element, and a custom element name may not
/// carry uppercase: valid XML, valid SVG, no legal HTML spelling.
#[test]
fn an_unwriteable_hyphenated_tag_fails_by_name() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><my-Icon/></svg>\n",
        "svg(\"/icons/i.svg\")",
    );
    let err = message(&site);
    assert!(err.contains("my-Icon"), "should name the tag: {err}");
    assert!(err.contains("uppercase"), "should explain: {err}");
}

/// A tag that is not even valid XML is the parser's to reject, so the message
/// points at the syntax rather than at HTML rules.
#[test]
fn an_invalid_xml_name_is_caught_while_parsing() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><-icon/></svg>\n",
        "svg(\"/icons/i.svg\")",
    );
    assert!(message(&site).contains("well-formed"), "{}", message(&site));
}

/// A reserved tag nested deep still fails: the walk is recursive, so a bad
/// element cannot hide inside a `<defs>`.
#[test]
fn a_reserved_tag_nested_deep_still_fails() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><defs><g><font-face/></g></defs></svg>\n",
        "svg(\"/icons/i.svg\")",
    );
    let err = message(&site);
    assert!(err.contains("font-face"), "{err}");
}

/// Malformed XML fails the build rather than shipping an empty `<svg>`.
#[test]
fn a_malformed_svg_fails_the_build() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle cx=\"1\"</svg>\n",
        "svg(\"/icons/i.svg\")",
    );
    let err = message(&site);
    assert!(err.contains("well-formed"), "{err}");
}

/// The path is read by the build, not by typst, so it must be project-absolute
/// and must not escape the project.
#[test]
fn a_path_outside_the_project_is_rejected() {
    for path in ["icons/i.svg", "/../i.svg", "/icons/../../i.svg"] {
        let site = icons(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n",
            &format!("svg({path:?})"),
        );
        let err = message(&site);
        assert!(err.contains("inside the project"), "{path}: {err}");
    }
}

/// A path that resolves but has no file names the file it wanted.
#[test]
fn a_missing_icon_names_itself() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n",
        "svg(\"/icons/gone.svg\")",
    );
    let err = message(&site);
    assert!(err.contains("/icons/gone.svg"), "{err}");
    assert!(err.contains("could not be read"), "{err}");
}

/// typst never reads an inlined icon, so the engine records it as a page
/// dependency itself.
#[test]
fn editing_an_icon_invalidates_the_page() {
    let site = icons(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"8\"/></svg>\n",
        "svg(\"/icons/i.svg\")",
    );
    assert_eq!(site.stats().cached, 0, "first build compiles");
    assert_eq!(site.stats().cached, 1, "unchanged icon stays cached");

    site.write(
        "icons/i.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"8\"/></svg>\n",
    );
    assert_eq!(site.stats().cached, 0, "an edited icon rebuilds the page");
    assert!(site.output("index.html").contains("<rect"), "redrawn");
}
