//! The `@baudelaire/*` virtual Typst modules: a template imports one and the
//! compiler resolves it from memory, with nothing on disk and no package
//! registry involved.

mod common;

use common::Site;

/// A failed build's diagnostics as one string. The top-level error only says
/// "typst compilation failed"; the message that matters is on the nested
/// per-source diagnostic.
fn diagnostics(site: &Site) -> String {
    format!("{:?}", site.build_error())
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

/// The headline case: named arguments become attributes, positional ones
/// children, and a hyphenated name needs no quoting (an `attrs` dict is always
/// a mix of bare and quoted keys otherwise).
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
/// attribute, `false` and `none` drop it, and anything else is coerced, so
/// neither a conditional attribute nor a number needs ceremony.
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

/// A misspelled module suggests the real one rather than sending the reader off
/// to install a package that was never meant to exist.
#[test]
fn an_unknown_module_suggests_the_nearest() {
    let site = site("import \"@baudelaire/htlm:0.1.0\": h\n[x]");
    let err = diagnostics(&site);
    assert!(err.contains("unknown baudelaire module `htlm`"), "{err}");
    assert!(err.contains("did you mean `html`?"), "{err}");
    assert!(err.contains("valid modules: html, site"), "{err}");
}

/// A version the registry does not serve fails at the import instead of
/// reaching for the network.
#[test]
fn an_unserved_version_is_rejected() {
    let site = site("import \"@baudelaire/html:9.9.9\": h\n[x]");
    let err = diagnostics(&site);
    assert!(err.contains("9.9.9"), "{err}");
}

/// A page importing a virtual module still caches: the module resolves to no
/// file, and a dependency that cannot be hashed must not be mistaken for one
/// that changed.
#[test]
fn a_page_importing_a_module_still_caches() {
    let site = site("import \"@baudelaire/html:0.1.0\": h\nh(\"p\")[x]");
    assert_eq!(site.stats().cached, 0, "first build compiles");
    assert_eq!(site.stats().cached, 1, "second build reuses");
}
