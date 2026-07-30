//! End-to-end tests for incremental builds and dependency tracking.
//!
//! Several of these exist to *confirm* a specific claim: that wrapping the
//! shared, comemo-memoized world in `Tracked` captures a page's true
//! dependency set (transitive imports and shared modules included) because
//! comemo re-calls the tracked `source`/`file` accessors when validating a
//! cached result. Each such test edits only a transitive/shared input and
//! asserts the affected page's *output* actually changed on rebuild; a missed
//! dependency would leave stale HTML and fail the test.

mod common;

use std::fs;

use common::{CONFIG, Site};

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

    site.stats();
    let second = site.stats();
    // Nothing changed -> every page served from cache.
    assert_eq!((second.pages, second.cached), (2, 2));
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
    site.stats();

    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nALPHA2",
    );
    let stats = site.stats();

    // One page recompiled, the other reused.
    assert_eq!((stats.pages, stats.cached), (2, 1));
    assert!(site.output("posts/a/index.html").contains("ALPHA2"));
}

#[test]
fn retitling_a_page_invalidates_its_sibling() {
    // A page's prev/next links carry its neighbour's title, baked into the
    // neighbour's layout wrapper. Retitling one must therefore rebuild the
    // sibling whose nav points at it; otherwise its "next" link goes stale.
    let site = Site::with(
        "site \"T\"\ncontent {\n  collections {\n    posts { template \"post.typ\" }\n  }\n}\n",
    );
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
    site.stats();
    assert!(site.output("posts/a/index.html").contains("B"));

    // Retitle b; a's next-link title must follow.
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"BEE\", order: 2,)\nbeta",
    );
    let stats = site.stats();
    assert!(
        site.output("posts/a/index.html").contains("BEE"),
        "sibling nav should reflect the neighbour's new title"
    );
    assert_eq!(
        stats.cached, 0,
        "retitling b must recompile a (its sibling nav changed)"
    );
}

#[test]
fn editing_transitive_import_invalidates_page() {
    // a imports b, b imports c. Editing only c must rebuild a, proving the
    // transitive dependency was captured.
    let site = Site::with(CONFIG);
    // Modules live at the project root so they aren't discovered as pages.
    site.write("c.typ", "#let value = \"ORIGINAL\"");
    site.write("b.typ", "#import \"/c.typ\": value\n#let msg = value");
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#import \"/b.typ\": msg\n#msg",
    );
    site.stats();
    assert!(site.output("posts/a/index.html").contains("ORIGINAL"));

    // Change only the leaf module.
    site.write("c.typ", "#let value = \"CHANGED\"");
    let stats = site.stats();

    assert!(
        site.output("posts/a/index.html").contains("CHANGED"),
        "transitive dep change did not propagate: c was not tracked as a dep of a"
    );
    assert_eq!(
        stats.cached, 0,
        "page a should have been recompiled, not cached"
    );
}

#[test]
fn shared_module_tracked_for_every_page() {
    // Two pages import the same module; within one build they share the
    // comemo-memoized world. Editing the module must rebuild BOTH, proving
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
    site.stats();
    assert!(site.output("posts/x/index.html").contains("V1"));
    assert!(site.output("posts/y/index.html").contains("V1"));

    site.write("shared.typ", "#let v = \"V2\"");
    let stats = site.stats();

    assert_eq!(stats.cached, 0, "neither page may be served from cache");
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
    site.stats();
    assert!(site.output("posts/a/index.html").contains("<main>"));

    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"section\", body)",
    );
    let stats = site.stats();
    assert!(
        site.output("posts/a/index.html").contains("<section>"),
        "template change did not invalidate the page"
    );
    assert_eq!(stats.cached, 0, "page should have rebuilt");
}

#[test]
fn generated_pages_are_cached() {
    // Taxonomy/pagination pages have synthetic sources that never touch disk.
    // Their fingerprint is the text typst compiles, so an unchanged rebuild must
    // reuse them alongside the real pages, not silently recompile every time.
    let site = Site::with(CONFIG);
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n\\\ncontent {\n  taxonomies {\n    tags listing=#true\n  }\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"x\",),)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", tags: (\"x\",),)\nbeta",
    );

    site.stats();
    let second = site.stats();
    // 2 posts + tags/index + tags/x = 4 pages, all served from cache.
    assert_eq!(
        (second.pages, second.cached),
        (4, 4),
        "generated pages must be cached on an unchanged rebuild"
    );
}

#[test]
fn retitling_invalidates_taxonomy_listing() {
    // A term listing embeds member titles, so retitling a member must rebuild
    // that listing, but not the index (its per-term counts are unchanged).
    let site = Site::with(CONFIG);
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n\\\ncontent {\n  taxonomies {\n    tags listing=#true\n  }\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"x\",),)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", tags: (\"x\",),)\nbeta",
    );
    site.stats();

    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"AA\", tags: (\"x\",),)\nalpha",
    );
    let stats = site.stats();

    assert!(
        site.output("tags/x/index.html").contains("AA"),
        "term listing did not pick up the new title"
    );
    // Page a rebuilds (the `#let frontmatter` export is part of its compiled
    // source: the page can render its own metadata) and so does the tags/x
    // listing that embeds the title. tags/index shows unchanged per-term counts
    // and stays cached; page b is untouched.
    assert_eq!((stats.pages, stats.cached), (4, 2));
}

#[test]
fn changing_a_slug_updates_links_from_cached_pages() {
    // Page a links to b by source path; b's permalink is resolved into a's HTML
    // at render time: a dependency the per-page tracker cannot see (typst never
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
    site.stats();
    assert!(
        site.output("posts/a/index.html").contains("/posts/b/"),
        "a's link should resolve to b's permalink"
    );

    // Give b a new slug -> new permalink.
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", slug: \"bee\",)\nbeta",
    );
    site.stats();

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
fn a_permalink_change_rebuilds_only_the_pages_that_link_to_it() {
    // The narrow half of the same claim: a's link to b is a dependency, so
    // moving b rebuilds a. c links to nobody, so moving b must leave it cached.
    // Keying the whole page-to-permalink map into the manifest fingerprint (as
    // this once did) rebuilds c too, which makes publishing a post cost a cold
    // build of the entire archive.
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#link(\"b.typ\")[to b]",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", slug: \"b\",)\nbeta",
    );
    site.write(
        "content/posts/c.typ",
        "#let frontmatter = (title: \"C\",)\ngamma",
    );
    site.stats();

    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\", slug: \"bee\",)\nbeta",
    );
    let stats = site.stats();

    // b recompiled (its own source changed) and a with it (its link moved); c
    // is untouched by either.
    assert_eq!((stats.pages, stats.cached), (3, 1));
    assert!(
        site.output("posts/a/index.html").contains("/posts/bee/"),
        "a's link should follow b"
    );
}

#[test]
fn adding_an_unrelated_page_leaves_every_other_page_cached() {
    // Publishing a post must not recompile the archive. Nothing here links to
    // the new page, so nothing here depends on it.
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    site.stats();

    site.write(
        "content/posts/c.typ",
        "#let frontmatter = (title: \"C\",)\ngamma",
    );
    let stats = site.stats();

    assert_eq!((stats.pages, stats.cached), (3, 2));
}

#[test]
fn a_link_to_a_page_that_does_not_exist_yet_resolves_when_it_appears() {
    // The negative dependency. `a` linked to a page that was not there, so it
    // recorded "nothing sits at b.typ". Writing b must invalidate a, or a stays
    // cached and serves the unresolved link forever.
    // A dangling link is a build error under the default `links { strict }`,
    // and the whole point here is to build with one and fix it afterwards.
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nlinks {\n  strict #false\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#link(\"b.typ\")[to b]",
    );
    site.stats();
    assert!(
        !site.output("posts/a/index.html").contains("/posts/b/"),
        "b does not exist yet, so nothing should resolve"
    );

    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    let stats = site.stats();

    assert_eq!(stats.cached, 0, "a must recompile now that b exists");
    assert!(
        site.output("posts/a/index.html").contains("/posts/b/"),
        "a's link should resolve once b appears: {}",
        site.output("posts/a/index.html")
    );
}

#[test]
#[cfg(unix)]
fn a_link_resolves_when_its_target_appears_under_a_symlinked_content_dir() {
    // The same negative dependency, with a symlinked content subtree (a shared
    // notes vault, a submodule linked into place). A page that exists
    // canonicalizes to its real location, while a probe at a page that is *not*
    // there has nothing to canonicalize and keeps the path as walked, so the two
    // used to be recorded under different keys: the recorded `None` kept
    // matching "nothing sits here", and `a` stayed a cache hit serving its
    // broken link.
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nlinks {\n  strict #false\n}\n",
    );
    fs::create_dir_all(site.path("vault/posts")).unwrap();
    fs::create_dir_all(site.path("content")).unwrap();
    std::os::unix::fs::symlink(site.path("vault/posts"), site.path("content/posts")).unwrap();

    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#link(\"b.typ\")[to b]",
    );
    site.stats();
    assert!(
        !site.output("posts/a/index.html").contains("/posts/b/"),
        "b does not exist yet, so nothing should resolve"
    );

    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    let stats = site.stats();

    assert_eq!(stats.cached, 0, "a must recompile now that b exists");
    assert!(
        site.output("posts/a/index.html").contains("/posts/b/"),
        "a's link should resolve once b appears: {}",
        site.output("posts/a/index.html")
    );
}

#[test]
fn a_link_that_falls_through_to_the_base_page_rebuilds_when_an_edition_appears() {
    // On a multilingual site a link probes the reader's own edition first and
    // falls through to the target as written. That fall-through is only correct
    // while the edition is absent, so its absence has to be a dependency too:
    // recording just the entry that matched leaves the French page pointing at
    // the English one after `b.fr.typ` is written.
    let site = Site::with(
        "site \"T\"\nlang \"en\"\nlanguages {\n  fr { name \"Français\" }\n}\npaths { content \"content\"; dist \"public\" }\n",
    );
    site.write(
        "content/posts/a.fr.typ",
        "#let frontmatter = (title: \"A\",)\n#link(\"b.typ\")[vers b]",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    site.stats();

    site.write(
        "content/posts/b.fr.typ",
        "#let frontmatter = (title: \"B\",)\nbeta en francais",
    );
    site.stats();

    let a = site.output("fr/posts/a/index.html");
    assert!(
        a.contains("/fr/posts/b/"),
        "a's link should follow the French edition once it exists: {a}"
    );
}

/// A site whose pages all render through `layout.typ`. `head` is prepended to
/// the template at file scope (for imports) and `extra` goes inside the layout
/// function's block, where the file is already in code mode.
fn templated(head: &str, extra: &str) -> Site {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  templates \"templates\"\n}\n",
    );
    site.write(
        "templates/layout.typ",
        &format!("{head}#let layout(page, body) = {{\n  body\n{extra}\n}}\n"),
    );
    // Five, so the page retitled below has pages that are *not* its prev/next
    // neighbours: those two rebuild for their pager links whatever happens, and
    // would mask what these tests are actually measuring.
    for (name, title) in [("a", "A"), ("b", "B"), ("c", "C"), ("d", "D"), ("e", "E")] {
        site.write(
            &format!("content/posts/{name}.typ"),
            &format!("#let frontmatter = (title: \"{title}\", template: \"layout.typ\",)\n{name}"),
        );
    }
    site
}

#[test]
fn retitling_leaves_pages_whose_template_ignores_the_section_tree_cached() {
    // The section tree names every page on the site. Embedded in each page's
    // generated wrapper (as it once was) it made every title part of every
    // page's fingerprint, so one retitle was a cold build of the whole site.
    // As a file, only the templates that import it depend on it.
    let site = templated("", "");
    site.stats();

    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B2\", template: \"layout.typ\",)\nb",
    );
    let stats = site.stats();

    // b recompiled, and a and c with it: they are its prev/next neighbours,
    // whose pager links carry its title. d and e are neither, and nothing here
    // reads the section tree, so they stay cached.
    assert_eq!((stats.pages, stats.cached), (5, 2));
}

#[test]
fn a_template_that_imports_the_section_tree_rebuilds_when_a_title_changes() {
    // The other half: a nav really does change on every page when any page is
    // retitled, so a template that renders one must rebuild. Correctness first;
    // the saving is only for templates that ask for nothing.
    let site = templated(
        "#import \"@baudelaire/sections:0.1.0\": sections\n",
        "  [#sections(page.lang).len()]",
    );
    site.stats();
    assert!(
        site.output("posts/a/index.html").contains('1'),
        "the tree should reach the template: {}",
        site.output("posts/a/index.html")
    );

    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B2\", template: \"layout.typ\",)\nb",
    );
    let stats = site.stats();

    assert_eq!(
        stats.cached, 0,
        "every page renders the retitled page's nav"
    );
}

#[test]
fn the_generated_section_tree_is_written_before_the_first_compile() {
    // A template importing the tree has to work on the first build of a fresh
    // checkout, where nothing has written it yet.
    let site = templated(
        "#import \"@baudelaire/sections:0.1.0\": sections\n",
        "  [#sections(page.lang).len()]",
    );

    let stats = site.stats();

    assert_eq!(stats.pages, 5);
    // Where the registry resolves `@baudelaire/sections` to. Asserted so the
    // import above is served from a real file, which is what puts it in each
    // importing page's dependency set.
    assert!(site.exists(".baudelaire/generated/sections.typ"));
}

#[test]
fn editing_an_embedded_asset_invalidates_the_page() {
    // With `embed`, a page inlines asset bytes as a `data:` URI at render time:
    // bytes typst never reads, so the per-page tracker is blind to them. Editing
    // the asset must still rebuild the page (its inlined copy is now stale).
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\n\\\nhtml {\n  embed #true\n}\n",
    );
    site.write("assets/note.svg", "<svg>ONE</svg>");
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#html.elem(\"img\", attrs: (src: \"/assets/note.svg\"))",
    );
    site.stats();
    let first = site.output("posts/a/index.html");
    assert!(
        first.contains("data:image/svg"),
        "asset should be inlined: {first}"
    );

    site.write("assets/note.svg", "<svg>TWO</svg>");
    let stats = site.stats();

    assert_eq!(
        stats.cached, 0,
        "page must rebuild when its embedded asset changes"
    );
    // The freshly inlined bytes differ from the original.
    assert_ne!(
        first,
        site.output("posts/a/index.html"),
        "inlined asset was not refreshed"
    );
}

#[test]
fn discovery_cache_persisted_and_reused() {
    // Discovery caches each page's extracted frontmatter so an unchanged rebuild
    // skips re-evaluating its module. The manifest must be written, and a second
    // build must produce identical output from it.
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", summary: \"hello\",)\nalpha",
    );
    site.stats();
    assert!(
        site.exists(".baudelaire/cache/discovery.json"),
        "discovery manifest should be written"
    );

    let before = site.output("posts/a/index.html");
    site.stats();
    // Frontmatter served from the discovery cache: output is unchanged.
    assert_eq!(before, site.output("posts/a/index.html"));
}

#[test]
fn frontmatter_from_import_invalidated_on_dep_change() {
    // A page's frontmatter reads a value from an imported module, so the cached
    // frontmatter depends on that module. Editing it must re-evaluate the page's
    // frontmatter: a missed dependency would serve the stale title from cache.
    let site = Site::with(
        "site \"T\"\ncontent {\n  collections {\n    posts { template \"post.typ\" }\n  }\n}\n",
    );
    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"html\", html.elem(\"body\", page.frontmatter.title))\n",
    );
    site.write("titles.typ", "#let title = \"FIRST\"");
    site.write(
        "content/posts/a.typ",
        "#import \"/titles.typ\": title\n#let frontmatter = (title: title,)\nbody",
    );
    site.stats();
    assert!(site.output("posts/a/index.html").contains("FIRST"));

    // Change only the imported module the frontmatter reads from.
    site.write("titles.typ", "#let title = \"SECOND\"");
    site.stats();
    assert!(
        site.output("posts/a/index.html").contains("SECOND"),
        "frontmatter dependency change did not propagate: the import was not \
         tracked as a discovery-cache dependency"
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
    let warm = site.build();
    // The flag is a `build` option, not a global one: passed before the
    // subcommand it is a usage error, and this test used to assert against
    // clap's help text rather than a build.
    assert!(
        warm.contains("cached"),
        "cache not warm to begin with: {warm}"
    );

    let out = site.run(&["build", "--no-cache", "-v"]);
    let logs = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "no-cache build failed: {logs}");
    // A forced full rebuild serves nothing from cache: no page nor the summary
    // mentions caching (the summary omits the cached count when it is zero).
    assert!(
        !logs.contains("cached"),
        "no-cache must rebuild everything: {logs}"
    );
}

/// A blob whose bytes no longer match its own content address is a miss, and
/// the build must repair it: it stays referenced (so the prune keeps it) and its
/// path exists (so a write is skipped), which left that page missing on every
/// build from then on.
#[test]
fn a_corrupt_blob_is_rewritten_not_missed_forever() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.stats();

    // Corrupt the one stored blob, leaving its content-addressed name intact.
    let objects = site.path(".baudelaire/cache/objects");
    let shard = fs::read_dir(&objects)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let blob = fs::read_dir(&shard)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&blob, "<html>tampered</html>").unwrap();

    let repaired = site.stats();
    assert_eq!(repaired.cached, 0, "a corrupt blob must be a miss");

    let healed = site.stats();
    assert_eq!(healed.cached, 1, "the rebuild did not repair the blob");
    assert!(site.output("posts/a/index.html").contains("alpha"));
}

/// A warm cache must survive the site moving on disk: `mv site site2`, or a CI
/// runner checking out at a different workspace path. Manifest keys are
/// therefore stored relative to the project root, not absolute.
#[test]
fn a_warm_cache_survives_the_site_moving() {
    fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let (src, dst) = (entry.path(), to.join(entry.file_name()));
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&src, &dst);
            } else {
                fs::copy(&src, &dst).unwrap();
            }
        }
    }

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

    let moved = tempfile::tempdir().unwrap();
    let root = moved.path().join("elsewhere");
    copy_tree(&site.root, &root);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_baudelaire"))
        .args(["build", "-v"])
        .current_dir(&root)
        .output()
        .expect("run binary");
    let logs = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "build failed after move: {logs}");
    assert!(
        logs.contains("2 cached"),
        "cache did not survive the move: {logs}"
    );
}

/// `datetime.today()` is a build input the file-dependency tracker cannot see:
/// it goes through the `World`, which records `source`/`file` only. Recording it
/// as a read of `sys.inputs.baudelaire.date` is what makes a page printing the
/// current year rebuild when the day rolls over instead of serving last year's
/// markup forever.
#[test]
fn a_page_reading_the_clock_records_the_date_as_a_dependency() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/clock.typ",
        "#let frontmatter = (title: \"C\",)\n#datetime.today().year()",
    );
    site.write(
        "content/posts/plain.typ",
        "#let frontmatter = (title: \"P\",)\nno clock here",
    );
    site.stats();

    let manifest = site.read(".baudelaire/cache/manifest.json");
    let entries: serde_json::Value = serde_json::from_str(&manifest).expect("manifest parses");
    let meta = |page: &str| {
        entries["pages"][format!("content/posts/{page}.typ")]["meta"]
            .as_object()
            .cloned()
            .unwrap_or_default()
    };
    assert!(
        meta("clock").contains_key("sys.inputs.baudelaire.date"),
        "clock read not recorded: {manifest}"
    );
    assert!(
        !meta("plain").contains_key("sys.inputs.baudelaire.date"),
        "a page that never asks for the date must not depend on it: {manifest}"
    );
}

/// `--no-cache` costs one cold build, not two: the manifest it writes must be
/// fingerprinted like any other, or the next normal build sees a mismatch and
/// rebuilds the whole site.
#[test]
fn no_cache_does_not_poison_the_next_build() {
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
    site.run(&["build", "--no-cache"]);

    let next = site.build();

    assert!(
        next.contains("2 cached"),
        "a --no-cache run left the cache unusable: {next}"
    );
}

// ---- Stale-output pruning -------------------------------------------------
//
// A build must not only write the current outputs; it must remove the ones a
// previous build wrote that no longer belong (a deleted page, a renamed
// permalink, a taxonomy term whose last page dropped it). Otherwise `dist`
// only grows and keeps serving files no source maps to. These lock in that
// pruning across the ways an output can be orphaned.

#[test]
fn deleted_page_is_pruned_from_dist() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    site.stats();
    assert!(site.exists("public/posts/b/index.html"));

    // Remove one source and rebuild: its output must not linger.
    fs::remove_file(site.path("content/posts/b.typ")).unwrap();
    site.stats();
    assert!(
        site.exists("public/posts/a/index.html"),
        "surviving page was wrongly pruned"
    );
    assert!(
        !site.exists("public/posts/b/index.html"),
        "deleted page's output was not pruned"
    );
    // The emptied directory should be gone too, not left as a husk.
    assert!(!site.exists("public/posts/b"), "empty dir left behind");
}

#[test]
fn renamed_page_prunes_the_old_permalink() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/old.typ",
        "#let frontmatter = (title: \"P\",)\nbody",
    );
    site.stats();
    assert!(site.exists("public/posts/old/index.html"));

    // Rename the source (slug -> permalink), which moves the output.
    fs::rename(
        site.path("content/posts/old.typ"),
        site.path("content/posts/new.typ"),
    )
    .unwrap();
    site.stats();
    assert!(
        site.exists("public/posts/new/index.html"),
        "renamed page's new output missing"
    );
    assert!(
        !site.exists("public/posts/old/index.html"),
        "old permalink was not pruned after rename"
    );
}

#[test]
fn dropped_taxonomy_term_prunes_its_index() {
    // The exact shape of the original bug: a term page lingering after no page
    // carries the term anymore.
    let config = format!("{CONFIG}content {{\n  taxonomies {{\n    tags listing=#true\n  }}\n}}\n");
    let site = Site::with(&config);
    site.write(
        "content/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"keep\", \"drop\"))\nhi",
    );
    site.stats();
    assert!(site.exists("public/tags/keep/index.html"));
    assert!(site.exists("public/tags/drop/index.html"));

    // The page keeps `keep` but loses `drop`; the `drop` term page must vanish.
    site.write(
        "content/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"keep\",))\nhi",
    );
    site.stats();
    assert!(
        site.exists("public/tags/keep/index.html"),
        "surviving term wrongly pruned"
    );
    assert!(
        !site.exists("public/tags/drop/index.html"),
        "orphaned taxonomy term page was not pruned"
    );
}

#[test]
fn changed_site_url_refreshes_canonical_on_rebuild() {
    // Regression for the sibling failure mode: a config change that only the
    // render pass sees (the base `url`, baked into canonical/og:url) must
    // invalidate cached pages, not serve their stale absolute links.
    let base = "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n";
    let site = Site::with(&format!("{base}url \"https://one.example\"\n"));
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.stats();
    assert!(
        site.output("posts/a/index.html")
            .contains("https://one.example/posts/a/"),
        "first build did not emit the configured canonical"
    );

    site.write(
        "config.kdl",
        &format!("{base}url \"https://two.example\"\n"),
    );
    site.stats();
    let html = site.output("posts/a/index.html");
    assert!(
        html.contains("https://two.example/posts/a/"),
        "canonical did not update after the url changed: cache served stale meta"
    );
    assert!(
        !html.contains("https://one.example"),
        "stale canonical from the old url survived the rebuild"
    );
}

#[test]
fn pruning_spares_assets_and_static_files() {
    let site = Site::with(CONFIG);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.write("assets/app.css", "body{color:red}");
    site.write("static/CNAME", "example.com");
    site.stats();
    assert!(
        site.exists("public/CNAME"),
        "static file missing after build"
    );
    assert!(!site.files("public/assets").is_empty(), "asset missing");

    // A no-op rebuild must not sweep away the asset tree or static passthrough.
    site.stats();
    assert!(
        site.exists("public/CNAME"),
        "static passthrough wrongly pruned"
    );
    assert!(
        !site.files("public/assets").is_empty(),
        "asset tree wrongly pruned"
    );
}

#[test]
fn clean_false_disables_pruning() {
    // The prune is opt-out: with `clean #false` an orphaned output survives, for
    // users who manage `dist/` by hand.
    let config = "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nprune #false\n";
    let site = Site::with(config);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    site.stats();
    assert!(site.exists("public/posts/b/index.html"));

    fs::remove_file(site.path("content/posts/b.typ")).unwrap();
    site.stats();
    assert!(
        site.exists("public/posts/b/index.html"),
        "clean #false must leave orphaned outputs in place"
    );
}

#[test]
fn flat_urls_still_prune_on_rename() {
    // Pruning is independent of URL style: a renamed page under flat URLs must
    // still drop its old `.html` output.
    let config = "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nlinks {\n  style \"flat\"\n}\n";
    let site = Site::with(config);
    site.write(
        "content/posts/old.typ",
        "#let frontmatter = (title: \"P\",)\nbody",
    );
    site.stats();
    assert!(site.exists("public/posts/old.html"));

    fs::rename(
        site.path("content/posts/old.typ"),
        site.path("content/posts/new.typ"),
    )
    .unwrap();
    site.stats();
    assert!(
        site.exists("public/posts/new.html"),
        "new flat output missing"
    );
    assert!(
        !site.exists("public/posts/old.html"),
        "old flat output not pruned"
    );
}

/// Run a git command in the site root, failing loudly. Isolated: the site lives
/// in a fresh tempdir, so this never touches the project's own repository.
fn git(site: &Site, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(&site.root)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn metadata_change_rebuilds_only_the_pages_that_read_it() {
    // The payoff of per-value tracking: a new commit changes
    // `sys.inputs.baudelaire.git.hash`, so the page that displays it rebuilds,
    // while a page that reads no metadata stays cached. Build metadata is not in
    // the manifest fingerprint, so nothing forces a whole-site rebuild.
    let site = Site::with(CONFIG);
    git(&site, &["init", "-q"]);
    git(&site, &["commit", "-q", "--allow-empty", "-m", "one"]);

    site.write(
        "content/reader.typ",
        "#let frontmatter = (title: \"R\",)\n\
         commit #sys.inputs.baudelaire.at(\"git\", default: (:)).at(\"hash\", default: \"none\")",
    );
    site.write(
        "content/plain.typ",
        "#let frontmatter = (title: \"P\",)\njust text",
    );

    site.stats();
    let reader_before = site.output("reader/index.html");
    assert!(
        !reader_before.contains("none"),
        "the reader page should display the real commit hash: {reader_before}"
    );

    // Unchanged rebuild: both pages served from cache.
    let unchanged = site.stats();
    assert_eq!(
        (unchanged.pages, unchanged.cached),
        (2, 2),
        "an unchanged rebuild must reuse both pages"
    );

    // A new commit changes only git.hash: no page source touched.
    git(&site, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let after = site.stats();

    // The reader rebuilt (its value changed); the plain page stayed cached.
    assert_eq!(
        (after.pages, after.cached),
        (2, 1),
        "exactly the metadata reader should rebuild"
    );
    assert_ne!(
        reader_before,
        site.output("reader/index.html"),
        "the reader page must show the new commit hash after a rebuild"
    );
}

/// The cache stays portable when `content` is not a direct child of the project
/// root: the root used to be inferred as `content`'s parent, so with
/// `paths { content "src/content" }` everything outside `src/` stored an
/// absolute path and the cache stopped surviving a move.
#[test]
fn a_nested_content_dir_keeps_the_cache_portable() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"src/content\"\n  templates \"templates\"\n  dist \"public\"\n}\n",
    );
    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"main\", body)\n",
    );
    site.write(
        "src/content/posts/a.typ",
        "#let frontmatter = (title: \"A\", template: \"post.typ\",)\nalpha",
    );
    site.stats();

    let manifest = site.read(".baudelaire/cache/manifest.json");
    assert!(
        !manifest.contains(site.root.to_str().expect("utf-8 tempdir")),
        "manifest stored an absolute project path: {manifest}"
    );
    assert!(manifest.contains("src/content/posts/a.typ"), "{manifest}");
    assert!(manifest.contains("templates/post.typ"), "{manifest}");
}

/// Asset fingerprinting used to rebuild every page whenever any asset changed,
/// because the whole request-to-served map was folded into the build
/// fingerprint. A page depends on the assets it actually references.
#[test]
#[cfg(feature = "css")]
fn changing_an_asset_leaves_pages_that_do_not_reference_it_cached() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  fingerprint #true\n}\n",
    );
    site.write("assets/used.css", "body{color:red}");
    site.write("assets/spare.css", "body{color:blue}");
    site.write(
        "content/uses.typ",
        "#let frontmatter = (title: \"Uses\",)\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/used.css\"))",
    );
    site.write(
        "content/plain.typ",
        "#let frontmatter = (title: \"Plain\",)\nnothing linked here",
    );
    site.stats();

    // Only the unreferenced asset changes, so only its (hashed) name moves.
    site.write("assets/spare.css", "body{color:green}");
    let stats = site.stats();

    assert_eq!(
        (stats.pages, stats.cached),
        (2, 2),
        "neither page references the changed asset"
    );

    // ...and the page that does reference an asset still follows it when it moves.
    site.write("assets/used.css", "body{color:black}");
    let stats = site.stats();
    assert_eq!(
        (stats.pages, stats.cached),
        (2, 1),
        "only the referencing page depends on the asset that moved"
    );
}

/// The negative half. A reference to an asset that is not there records that it
/// was absent, so the page picks up the real URL once the asset appears.
/// Recording only the references that resolved would leave it cached pointing
/// at a file that never existed.
#[test]
#[cfg(feature = "css")]
fn a_reference_to_an_asset_that_appears_later_invalidates_the_page() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  fingerprint #true\n}\n",
    );
    site.write("assets/present.css", "body{color:red}");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/later.css\"))",
    );
    site.stats();
    assert!(
        site.output("index.html").contains("/assets/later.css"),
        "nothing is mapped there yet, so the reference stays as authored"
    );

    site.write("assets/later.css", "body{color:blue}");
    site.stats();

    let html = site.output("index.html");
    assert!(
        !html.contains("\"/assets/later.css\""),
        "the reference must follow the asset now that it exists: {html}"
    );
}

/// The complement of `editing_an_embedded_asset_invalidates_the_page`: the
/// embedded bytes are now the embedding page's own dependency, so a page that
/// inlines nothing is untouched by an asset edit. Hashing the whole staged
/// asset tree (as this once did) rebuilt every page, and walked the tree on
/// every build to do it.
#[test]
fn a_page_that_embeds_nothing_survives_an_asset_edit() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nhtml {\n  embed #true\n}\n",
    );
    site.write("assets/inlined.css", "body{color:red}");
    site.write(
        "content/embeds.typ",
        "#let frontmatter = (title: \"Embeds\",)\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/inlined.css\"))",
    );
    site.write(
        "content/plain.typ",
        "#let frontmatter = (title: \"Plain\",)\nnothing inlined here",
    );
    site.stats();

    site.write("assets/inlined.css", "body{color:blue}");
    let stats = site.stats();

    assert_eq!(
        (stats.pages, stats.cached),
        (2, 1),
        "only the page carrying the bytes depends on them"
    );
    assert!(
        site.output("embeds/index.html").contains("data:text/css"),
        "the embedding page still inlines the asset"
    );
}
