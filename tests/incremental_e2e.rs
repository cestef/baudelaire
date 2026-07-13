//! End-to-end tests for incremental builds and dependency tracking.
//!
//! Several of these exist to *confirm* a specific claim: that wrapping the
//! shared, comemo-memoized world in `Tracked` captures a page's true
//! dependency set — transitive imports and shared modules included — because
//! comemo re-calls the tracked `source`/`file` accessors when validating a
//! cached result. Each such test edits only a transitive/shared input and
//! asserts the affected page's *output* actually changed on rebuild; a missed
//! dependency would leave stale HTML and fail the test.

mod common;

use common::Site;

const CONFIG: &str =
    "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n";

#[test]
fn second_build_reuses_all_pages() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );

    site.build();
    let second = site.build();
    // Nothing changed → every page served from cache.
    assert!(
        second.contains("(2 cached)"),
        "expected all pages cached: {second}"
    );
}

#[test]
fn corrupt_manifest_warns_and_rebuilds() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.build();

    // A present-but-unparseable manifest must not be mistaken for a fresh cache.
    site.write(".baudelaire/cache/manifest.json", "{ not valid json");
    let out = site.run(&["build", "-v"]);
    assert!(
        out.status.success(),
        "build should self-heal past a corrupt manifest: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let logs = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        logs.contains("unreadable cache manifest"),
        "expected a warning: {logs}"
    );
}

#[test]
fn editing_a_page_rebuilds_only_it() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    site.build();

    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nALPHA2",
    );
    let out = site.build();

    // One page recompiled, the other reused.
    assert!(out.contains("(1 cached)"), "expected 1 cached: {out}");
    assert!(site.output("posts/a/index.html").contains("ALPHA2"));
}

#[test]
fn retitling_a_page_invalidates_its_sibling() {
    // A page's prev/next links carry its neighbour's title, baked into the
    // neighbour's layout wrapper. Retitling one must therefore rebuild the
    // sibling whose nav points at it — otherwise its "next" link goes stale.
    let site = Site::with("site \"T\"\ncollections {\n  posts template=\"post.typ\"\n}\nclean #true\n");
    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"html\", html.elem(\"body\", {\n  body\n  if page.nav.next != none { html.elem(\"a\", attrs: (href: page.nav.next.url), page.nav.next.title) }\n}))\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", order: 1,)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", order: 2,)\nbeta",
    );
    site.build();
    assert!(site.output("posts/a/index.html").contains("B"));

    // Retitle b; a's next-link title must follow.
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"BEE\", order: 2,)\nbeta",
    );
    let out = site.build();
    assert!(
        site.output("posts/a/index.html").contains("BEE"),
        "sibling nav should reflect the neighbour's new title"
    );
    assert!(
        !out.contains("(1 cached)"),
        "retitling b must recompile a (its sibling nav changed): {out}"
    );
}

#[test]
fn editing_transitive_import_invalidates_page() {
    // a imports b, b imports c. Editing only c must rebuild a — proving the
    // transitive dependency was captured.
    let site = Site::with(CONFIG);
    // Modules live at the project root so they aren't discovered as pages.
    site.write("c.typ", "#let value = \"ORIGINAL\"");
    site.write("b.typ", "#import \"/c.typ\": value\n#let msg = value");
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#import \"/b.typ\": msg\n#msg",
    );
    site.build();
    assert!(site.output("posts/a/index.html").contains("ORIGINAL"));

    // Change only the leaf module.
    site.write("c.typ", "#let value = \"CHANGED\"");
    let out = site.build();

    assert!(
        site.output("posts/a/index.html").contains("CHANGED"),
        "transitive dep change did not propagate — c was not tracked as a dep of a"
    );
    assert!(
        !out.contains("(1 cached)"),
        "page a should have been recompiled, not cached: {out}"
    );
}

#[test]
fn shared_module_tracked_for_every_page() {
    // Two pages import the same module; within one build they share the
    // comemo-memoized world. Editing the module must rebuild BOTH — proving
    // the dependency is recorded even when the second page's compile reuses
    // comemo's cached evaluation of the shared module.
    let site = Site::with(CONFIG);
    site.write("shared.typ", "#let v = \"V1\"");
    site.write(
        "content/posts/x.typ",
        "#let frontmatter = (title: \"X\",)\n#import \"/shared.typ\": v\n#v",
    );
    site.write(
        "content/posts/y.typ",
        "#let frontmatter = (title: \"Y\",)\n#import \"/shared.typ\": v\n#v",
    );
    site.build();
    assert!(site.output("posts/x/index.html").contains("V1"));
    assert!(site.output("posts/y/index.html").contains("V1"));

    site.write("shared.typ", "#let v = \"V2\"");
    site.build();

    assert!(
        site.output("posts/x/index.html").contains("V2"),
        "x not invalidated by shared module change"
    );
    assert!(
        site.output("posts/y/index.html").contains("V2"),
        "y not invalidated by shared module change (comemo re-validation claim is false)"
    );
}

#[test]
fn editing_layout_template_rebuilds_dependent_pages() {
    // The layout template is imported through the tracked world, so editing it
    // must invalidate every page bound to it.
    let site = Site::with(CONFIG);
    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"main\", body)",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", template: \"post.typ\",)\nalpha",
    );
    site.build();
    assert!(site.output("posts/a/index.html").contains("<main>"));

    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"section\", body)",
    );
    let out = site.build();
    assert!(
        site.output("posts/a/index.html").contains("<section>"),
        "template change did not invalidate the page"
    );
    assert!(
        !out.contains("(1 cached)"),
        "page should have rebuilt: {out}"
    );
}

#[test]
fn generated_pages_are_cached() {
    // Taxonomy/pagination pages have synthetic sources that never touch disk.
    // Their fingerprint is the text typst compiles, so an unchanged rebuild must
    // reuse them alongside the real pages — not silently recompile every time.
    let site = Site::with(CONFIG);
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n\
         taxonomies {\n  tags index=#true\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"x\",),)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", tags: (\"x\",),)\nbeta",
    );

    site.build();
    let second = site.build();
    // 2 posts + tags/index + tags/x = 4 pages, all served from cache.
    assert!(
        second.contains("(4 cached)"),
        "generated pages must be cached on an unchanged rebuild: {second}"
    );
}

#[test]
fn retitling_invalidates_taxonomy_listing() {
    // A term listing embeds member titles, so retitling a member must rebuild
    // that listing — but not the index (its per-term counts are unchanged).
    let site = Site::with(CONFIG);
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n\
         taxonomies {\n  tags index=#true\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"x\",),)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", tags: (\"x\",),)\nbeta",
    );
    site.build();

    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"AA\", tags: (\"x\",),)\nalpha",
    );
    let out = site.build();

    assert!(
        site.output("tags/x/index.html").contains("AA"),
        "term listing did not pick up the new title"
    );
    // Page a rebuilds (the `#let frontmatter` export is part of its compiled
    // source — the page can render its own metadata) and so does the tags/x
    // listing that embeds the title. tags/index shows unchanged per-term counts
    // and stays cached; page b is untouched.
    assert!(out.contains("(2 cached)"), "expected 2 cached: {out}");
}

#[test]
fn changing_a_slug_updates_links_from_cached_pages() {
    // Page a links to b by source path; b's permalink is resolved into a's HTML
    // at render time — a dependency the per-page tracker cannot see (typst never
    // reads b.typ). Changing b's slug must still invalidate a's cached link.
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#link(\"b.typ\")[to b]",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", slug: \"b\",)\nbeta",
    );
    site.build();
    assert!(
        site.output("posts/a/index.html").contains("/posts/b/"),
        "a's link should resolve to b's permalink"
    );

    // Give b a new slug → new permalink.
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", slug: \"bee\",)\nbeta",
    );
    site.build();

    let a = site.output("posts/a/index.html");
    assert!(
        a.contains("/posts/bee/"),
        "a's link was not updated to b's new slug: {a}"
    );
    assert!(
        !a.contains("/posts/b/\""),
        "a still serves b's stale permalink: {a}"
    );
}

#[test]
fn editing_an_embedded_asset_invalidates_the_page() {
    // With `embed`, a page inlines asset bytes as a `data:` URI at render time —
    // bytes typst never reads, so the per-page tracker is blind to them. Editing
    // the asset must still rebuild the page (its inlined copy is now stale).
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\n\
         clean #true\noutput {\n  html {\n    embed #true\n  }\n}\n",
    );
    site.write("assets/note.svg", "<svg>ONE</svg>");
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#html.elem(\"img\", attrs: (src: \"/assets/note.svg\"))",
    );
    site.build();
    let first = site.output("posts/a/index.html");
    assert!(
        first.contains("data:image/svg"),
        "asset should be inlined: {first}"
    );

    site.write("assets/note.svg", "<svg>TWO</svg>");
    let out = site.build();

    assert!(
        !out.contains("(1 cached)"),
        "page must rebuild when its embedded asset changes: {out}"
    );
    // The freshly inlined bytes differ from the original.
    assert_ne!(
        first,
        site.output("posts/a/index.html"),
        "inlined asset was not refreshed"
    );
}

#[test]
fn no_cache_flag_forces_full_rebuild() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.build();

    let out = site.run(&["--no-cache", "build", "-v"]);
    let logs = String::from_utf8_lossy(&out.stderr);
    // A forced full rebuild serves nothing from cache — no page nor the summary
    // mentions caching (the summary omits the cached count when it is zero).
    assert!(
        !logs.contains("cached"),
        "no-cache must rebuild everything: {logs}"
    );
}
