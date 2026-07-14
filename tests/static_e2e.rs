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
    clean #true
"#;

fn built(site: &Site) {
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn copies_files_verbatim_to_dist_root() {
    let site = Site::with(CONFIG);
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhi\n",
    );
    site.write("static/install.sh", "#!/bin/sh\necho hi\n");
    built(&site);
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
    built(&site);
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
    built(&site);
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
    built(&site);
    assert!(site.exists("public/index.html"));
}
