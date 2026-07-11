//! End-to-end tests for incremental builds and dependency tracking.
//!
//! Several of these exist to *confirm* a specific claim: that wrapping the
//! shared, comemo-memoized world in `Tracked` captures a page's true
//! dependency set — transitive imports and shared modules included — because
//! comemo re-calls the tracked `source`/`file` accessors when validating a
//! cached result. Each such test edits only a transitive/shared input and
//! asserts the affected page's *output* actually changed on rebuild; a missed
//! dependency would leave stale HTML and fail the test.

use std::fs;
use std::process::Command;

struct Site {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

impl Site {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let site = Self { _tmp: tmp, root };
        site.write(
            "config.kdl",
            "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
        );
        site
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn build(&self) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_baudelaire"))
            .args(["build", "-v"])
            .current_dir(&self.root)
            .output()
            .expect("run binary");
        assert!(
            out.status.success(),
            "build failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn output(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join("public").join(rel)).unwrap()
    }
}

#[test]
fn second_build_reuses_all_pages() {
    let site = Site::new();
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nalpha");
    site.write("content/posts/b.typ", "#frontmatter((title: \"B\",))\nbeta");

    site.build();
    let second = site.build();
    // Nothing changed → every page served from cache.
    assert!(
        second.contains("(2 cached)"),
        "expected all pages cached: {second}"
    );
}

#[test]
fn editing_a_page_rebuilds_only_it() {
    let site = Site::new();
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nalpha");
    site.write("content/posts/b.typ", "#frontmatter((title: \"B\",))\nbeta");
    site.build();

    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nALPHA2");
    let out = site.build();

    // One page recompiled, the other reused.
    assert!(out.contains("(1 cached)"), "expected 1 cached: {out}");
    assert!(site.output("posts/a/index.html").contains("ALPHA2"));
}

#[test]
fn editing_transitive_import_invalidates_page() {
    // a imports b, b imports c. Editing only c must rebuild a — proving the
    // transitive dependency was captured.
    let site = Site::new();
    // Modules live at the project root so they aren't discovered as pages.
    site.write("c.typ", "#let value = \"ORIGINAL\"");
    site.write("b.typ", "#import \"/c.typ\": value\n#let msg = value");
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\",))\n#import \"/b.typ\": msg\n#msg",
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
    let site = Site::new();
    site.write("shared.typ", "#let v = \"V1\"");
    site.write(
        "content/posts/x.typ",
        "#frontmatter((title: \"X\",))\n#import \"/shared.typ\": v\n#v",
    );
    site.write(
        "content/posts/y.typ",
        "#frontmatter((title: \"Y\",))\n#import \"/shared.typ\": v\n#v",
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
    let site = Site::new();
    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"main\", body)",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\", template: \"post.typ\",))\nalpha",
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
    assert!(!out.contains("(1 cached)"), "page should have rebuilt: {out}");
}

#[test]
fn generated_pages_are_cached() {
    // Taxonomy/pagination pages have synthetic sources that never touch disk.
    // Their fingerprint is the text typst compiles, so an unchanged rebuild must
    // reuse them alongside the real pages — not silently recompile every time.
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n\
         taxonomies {\n  tags index=#true\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\", tags: (\"x\",),))\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#frontmatter((title: \"B\", tags: (\"x\",),))\nbeta",
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
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n\
         taxonomies {\n  tags index=#true\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\", tags: (\"x\",),))\nalpha",
    );
    site.write(
        "content/posts/b.typ",
        "#frontmatter((title: \"B\", tags: (\"x\",),))\nbeta",
    );
    site.build();

    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"AA\", tags: (\"x\",),))\nalpha",
    );
    let out = site.build();

    assert!(
        site.output("tags/x/index.html").contains("AA"),
        "term listing did not pick up the new title"
    );
    // Only the tags/x listing rebuilds. Page a itself is template-less, so its
    // HTML never embedded the title and stays cached; tags/index shows unchanged
    // per-term counts and stays cached; page b is untouched.
    assert!(out.contains("(3 cached)"), "expected 3 cached: {out}");
}

#[test]
fn no_cache_flag_forces_full_rebuild() {
    let site = Site::new();
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nalpha");
    site.build();

    let out = Command::new(env!("CARGO_BIN_EXE_baudelaire"))
        .args(["--no-cache", "build", "-v"])
        .current_dir(&site.root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // A forced full rebuild serves nothing from cache — no page nor the summary
    // mentions caching (the summary omits the cached count when it is zero).
    assert!(
        !stdout.contains("cached"),
        "no-cache must rebuild everything: {stdout}"
    );
}
