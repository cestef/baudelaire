mod common;

use common::Site;

/// The standard config plus a `static` passthrough dir alongside content.
const CONFIG: &str = r#"
    site "T"
    paths {
        content "content"
        dist "public"
        static "static"
    }
"#;

#[test]
fn copies_files_verbatim_to_dist_root() {
    let site = Site::with(CONFIG);
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhi\n",
    );
    site.write("static/install.sh", "#!/bin/sh\necho hi\n");
    site.stats();
    // Same path at the site root, byte-identical, name untouched (no fingerprint).
    assert_eq!(site.read("public/install.sh"), "#!/bin/sh\necho hi\n");
}

#[test]
fn preserves_nested_layout() {
    let site = Site::with(CONFIG);
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhi\n",
    );
    site.write("static/.well-known/security.txt", "Contact: x@y.z\n");
    site.stats();
    assert_eq!(
        site.read("public/.well-known/security.txt"),
        "Contact: x@y.z\n"
    );
}

#[test]
fn a_generated_page_overrides_a_static_file() {
    let site = Site::with(CONFIG);
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\ngenerated\n",
    );
    // A static index.html at the same output path must lose to the page.
    site.write("static/index.html", "STATIC");
    site.stats();
    let html = site.read("public/index.html");
    assert!(html.contains("generated"), "page should win: {html}");
    assert!(!html.contains("STATIC"));
}

#[test]
fn missing_static_dir_is_not_an_error() {
    let site = Site::with(CONFIG);
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhi\n",
    );
    // No `static/` directory at all.
    site.stats();
    assert!(site.exists("public/index.html"));
}

/// `static/` is the documented override escape hatch, but processors run after
/// the static copy, so with `sitemap` on by default a hand-authored
/// `static/sitemap.xml` was overwritten on every build. Only whole-site derived
/// files yield; a page still wins, see
/// `a_generated_page_overrides_a_static_file`.
#[test]
fn a_static_file_overrides_generated_processor_output() {
    // The sitemap processor has to actually run, so this needs `url` *and* the
    // opt-in: without them nothing would compete with the static copy and the
    // test would pass with the override deleted.
    let site = Site::with(
        r#"
        site "T"
        url "https://host.test"
        paths {
          content "content"
          dist "public"
          static "static"
        }
        generate {
          sitemap #true
        }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhi\n",
    );
    site.write("static/sitemap.xml", "<!-- mine -->\n");
    site.stats();
    assert_eq!(site.read("public/sitemap.xml"), "<!-- mine -->\n");
}

/// A static file inside the asset directory survives. The pipeline replaces
/// `dist/assets` wholesale when it publishes, so a file the static copy had
/// already written there was deleted with the directory it replaced.
#[test]
fn a_static_file_under_the_asset_dir_survives_the_pipeline() {
    let site = Site::with(
        r#"
        site "T"
        paths {
            content "content"
            dist "public"
            assets "assets"
            static "static"
        }
    "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhi\n",
    );
    site.write("assets/app.css", "body { color: red }\n");
    site.write("static/assets/vendor.js", "// vendored\n");
    site.stats();

    assert_eq!(site.read("public/assets/vendor.js"), "// vendored\n");
    assert!(
        site.exists("public/assets/app.css"),
        "pipeline output kept too"
    );
}

/// A failed build leaves no staged asset tree in `dist`: `deploy` walks the
/// whole directory and would upload it as a duplicate copy of the assets.
#[test]
fn a_failed_build_leaves_no_staging_tree() {
    let site = Site::with(
        r#"
        site "T"
        paths {
            content "content"
            dist "public"
            assets "assets"
            templates "templates"
        }
    "#,
    );
    site.write("assets/app.css", "body { color: red }\n");
    site.write("templates/broken.typ", "#let broken(page, body) = #(\n");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\", template: \"broken.typ\",)\nx\n",
    );
    site.build_error();

    let leftovers: Vec<_> = site
        .files("public")
        .into_iter()
        .filter(|name| name.contains("staging"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "staging tree left behind: {leftovers:?}"
    );
}
