use crate::common::Site;

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
    site.stats();
    assert!(site.exists("public/index.html"));
}

/// Only whole-site derived files yield to `static/`; a page still wins, see
/// `a_generated_page_overrides_a_static_file`.
#[test]
fn a_static_file_overrides_generated_processor_output() {
    // The sitemap processor must actually run, so `url` and the opt-in are
    // both needed or nothing competes with the static copy.
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

/// `deploy` walks the whole `dist` directory, so a leftover staging tree would
/// be uploaded as a second copy of the assets.
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
