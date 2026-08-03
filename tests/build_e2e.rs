//! The end-to-end builds that a scenario cannot express.
//!
//! The bulk of what used to live here is now data under `tests/scenarios/`; see
//! `tests/scenarios.rs` for the format and for why each of these stayed. In
//! short, they need something a scenario has no vocabulary for: the CLI's own
//! stderr, the cache's internal files, or the library's own return values.
//!
//! Several of the old reasons are gone: a generated name fed into a second
//! assertion is a `capture`, byte-level comparisons are `identical`/`smaller`,
//! and several builds in a row are `build { }` steps.

mod common;

use std::fs;

use baudelaire::content::discover;

use common::{Site, has_ext, project};

#[test]
fn check_compiles_without_writing() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
        "#,
    );
    site.write(
        "content/posts/ok.typ",
        "#let frontmatter = (title: \"OK\",)\nfine",
    );
    let out = site.run(&["check"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !site.root.join("public").exists(),
        "check must not write output"
    );
}

#[test]
fn check_fails_on_compile_error() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
        "#,
    );
    site.write("content/posts/bad.typ", "#html.frame[\n  #nope\n]");
    let out = site.run(&["check"]);
    assert!(!out.status.success());
}

#[test]
fn compile_error_reports_with_context() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
        "#,
    );
    site.write("content/posts/bad.typ", "#html.frame[\n  #invalid_func\n]");
    let out = site.run(&["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The context is the point: which file, which line, and the offending text.
    assert!(stderr.contains("bad.typ"), "no source named: {stderr}");
    assert!(stderr.contains("invalid_func"), "no span excerpt: {stderr}");
    assert!(stderr.contains("bad.typ:2"), "no line number: {stderr}");
}

#[test]
fn error_in_a_bound_template_renders_against_the_template_file() {
    // A span reaching into another file (here a template, whose text differs in
    // length from the page's) must resolve against that file: never overrun the
    // page source and crash the renderer with an `OutOfBounds` panic.
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\ncontent {\n  collections { pages { template \"page.typ\" } }\n}\n",
    );
    // Padding pushes the erroring span past the length of the short page source.
    site.write(
        "templates/page.typ",
        "#let pad = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n#let page(meta, body) = html.elem(\"html\", html.elem(\"body\", { body; nope_undefined }))\n",
    );
    site.write(
        "content/pages/a.typ",
        "#let frontmatter = (title: \"A\",)\nhi",
    );
    let out = site.run(&["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("OutOfBounds"),
        "renderer panicked: {stderr}"
    );
    assert!(stderr.contains("unknown variable"), "{stderr}");
    // The label lands in the template, not the page.
    assert!(
        stderr.contains("page.typ"),
        "points at the template: {stderr}"
    );
}

#[test]
fn build_summary_reports_assets_generated_files_and_output_dir() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://x.example\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\ngenerate {\n  search { formats \"json\" }\n}\n",
    );
    site.write("assets/style.css", "body { color: red; }");
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nbody");
    // The summary is CLI output, so this one stays on the real binary.
    let logs = site.build();
    // The compact summary line counts assets and generated files, and shows dist.
    assert!(logs.contains("1 asset"), "assets counted: {logs}");
    assert!(logs.contains("file"), "generated files counted: {logs}");
    assert!(logs.contains("╰─ public"), "output dir shown: {logs}");
}

/// The orphan report is only worth having if it names the right pages: the one
/// nothing points at, and not the one every layout points at. A report that
/// counted the nav would name nothing at all on a site with a nav, which is
/// every site.
#[test]
fn the_orphan_report_names_the_page_no_content_links_to() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nlinks {\n  orphans \"any\"\n}\npaths {\n  content \"content\"\n  templates \"templates\"\n  dist \"public\"\n}\n",
    );
    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"main\", \
         link(\"/content/lonely.typ\")[nav] + body)\n",
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"post.typ\",)\n#link(\"seen.typ\")[seen]",
    );
    site.write(
        "content/seen.typ",
        "#let frontmatter = (title: \"Seen\", template: \"post.typ\",)\nlinked from home",
    );
    site.write(
        "content/lonely.typ",
        "#let frontmatter = (title: \"Lonely\", template: \"post.typ\",)\nlinked from the layout",
    );

    let logs = site.build();

    assert!(
        logs.contains("/lonely/"),
        "the orphan says where it is served: {logs}"
    );
    // Named as *orphans*, specifically: every page also appears in the build's
    // own progress lines.
    let orphaned = |page: &str| logs.contains(&format!("`{page}` is linked from nowhere"));
    assert!(orphaned("lonely.typ"), "{logs}");
    assert!(
        !orphaned("seen.typ"),
        "a page linked from content is not an orphan: {logs}"
    );
    // The home page is an entry point, not a page anyone forgot to link.
    assert!(!orphaned("index.typ"), "the root is not one: {logs}");
}

#[cfg(feature = "images")]
#[test]
fn optimize_losslessly_shrinks_png_assets() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  images { optimize { png } }\n}\n",
    );
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nbody");
    // A PNG bloated with strippable metadata and a stored (uncompressed) IDAT.
    let png = include_bytes!("fixtures/bloated.png");
    site.write_bytes("assets/pic.png", png);
    site.stats();

    let out = fs::read(site.root.join("public/assets/pic.png")).unwrap();
    assert!(
        out.len() < png.len(),
        "optimized {} < original {}",
        out.len(),
        png.len()
    );
    // Still a valid PNG (signature intact).
    assert_eq!(&out[..8], b"\x89PNG\r\n\x1a\n", "output is a PNG");
}

#[cfg(feature = "images")]
#[test]
fn optimize_reencodes_jpeg_with_lax_extension() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  images { optimize { jpeg quality=70 } }\n}\n",
    );
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nbody");
    // A high-quality JPEG shrinks when re-encoded at quality 70. The `.jpg`
    // extension must match the `jpeg` format leniently.
    let jpg = include_bytes!("fixtures/big.jpg");
    site.write_bytes("assets/photo.jpg", jpg);
    site.stats();

    let out = fs::read(site.root.join("public/assets/photo.jpg")).unwrap();
    assert!(
        out.len() < jpg.len(),
        "re-encoded {} < original {}",
        out.len(),
        jpg.len()
    );
    assert_eq!(&out[..2], b"\xff\xd8", "output is a JPEG");
}

#[test]
fn nested_dirs_traverse_and_build() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
        "#,
    );
    site.write(
        "content/posts/2024/jan.typ",
        "#let frontmatter = (title: \"Jan\",)\nJanuary",
    );
    site.write(
        "content/posts/2024/feb.typ",
        "#let frontmatter = (title: \"Feb\",)\nFebruary",
    );
    site.stats();
    let cols = discover(&site.config(), &project(&site.config())).unwrap();
    let posts = cols.iter().find(|c| c.id == "posts").unwrap();
    assert_eq!(posts.pages.len(), 2);
}

#[cfg(feature = "css")]
#[test]
fn embed_inlines_processed_not_source_bytes() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  minify #true\n}\nhtml {\n  embed #true\n}\n",
    );
    // A comment survives only in the raw source; minification drops it.
    site.write(
        "assets/style.css",
        "/* source-only comment */\nbody {\n  color: red;\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/style.css\"))",
    );
    site.stats();
    let html = fs::read_to_string(site.root.join("public/posts/a/index.html")).unwrap();
    let marker = "data:text/css;base64,";
    let start = html.find(marker).expect("data uri present") + marker.len();
    let b64: String = html[start..].chars().take_while(|c| *c != '"').collect();
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("printf %s '{b64}' | base64 -d"))
        .output()
        .expect("base64");
    let decoded = String::from_utf8_lossy(&out.stdout);
    // The inlined bytes are the minified output, not the raw source.
    assert!(
        !decoded.contains("/*"),
        "inlined raw source, not processed: {decoded}"
    );
    assert!(decoded.contains("red"), "declaration lost: {decoded}");
}

#[test]
fn build_context_exposed_via_sys_inputs() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    site.write(
        "content/posts/v.typ",
        "#let frontmatter = (title: \"V\", slug: \"v\",)\nversion=#sys.inputs.baudelaire.version site=#sys.inputs.baudelaire.site.title mode=#sys.inputs.baudelaire.mode",
    );
    site.stats();
    let html = fs::read_to_string(site.root.join("public/posts/v/index.html")).unwrap();
    assert!(
        html.contains(&format!("version={}", env!("CARGO_PKG_VERSION"))),
        "{html}"
    );
    assert!(html.contains("site=T"), "site mirror exposed: {html}");
    assert!(html.contains("mode=build"), "build mode exposed: {html}");
}

#[test]
fn srcset_urls_are_fingerprinted() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  fingerprint #true\n}\n",
    );
    site.write("assets/a.png", "AAA");
    site.write("assets/b.png", "BBB");
    site.write(
        "content/a.typ",
        "#let frontmatter = (title: \"A\",)\n#html.elem(\"img\", attrs: (srcset: \"/assets/a.png 1x, /assets/b.png 2x\"))",
    );
    site.stats();
    let names = site.files("public/assets");
    let a = names
        .iter()
        .find(|n| n.starts_with("a.") && has_ext(n, "png"))
        .expect("hashed a");
    let b = names
        .iter()
        .find(|n| n.starts_with("b.") && has_ext(n, "png"))
        .expect("hashed b");
    let html = fs::read_to_string(site.root.join("public/a/index.html")).unwrap();
    // Each candidate URL is fingerprinted; its descriptor is preserved.
    assert!(
        html.contains(&format!("/assets/{a} 1x")),
        "srcset a not rewritten: {html}"
    );
    assert!(
        html.contains(&format!("/assets/{b} 2x")),
        "srcset b not rewritten: {html}"
    );
}

#[test]
fn cache_stores_html_in_object_store_not_manifest() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"Unique Marker\",)\nDistinct Body Text",
    );
    site.stats();
    // The manifest is metadata only: page markup lives in the object store.
    let manifest = fs::read_to_string(site.root.join(".baudelaire/cache/manifest.json")).unwrap();
    assert!(
        !manifest.contains("Distinct Body Text"),
        "no html in manifest: {manifest}"
    );
    assert!(
        manifest.contains("blob"),
        "manifest points at blobs: {manifest}"
    );
    // At least one content-addressed blob was written, holding the HTML.
    let objects = site.root.join(".baudelaire/cache/objects");
    assert!(objects.is_dir(), "object store created");
    let shard = fs::read_dir(&objects)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("a sharded object directory");
    let blob = fs::read_dir(&shard)
        .unwrap()
        .find_map(std::result::Result::ok)
        .expect("a blob file");
    let html = fs::read_to_string(blob.path()).unwrap();
    assert!(
        html.contains("Distinct Body Text"),
        "blob holds html: {html}"
    );
}

#[test]
fn before_hook_output_flows_into_the_asset_pipeline() {
    // The Tailwind model: a before hook generates CSS into assets/, which the
    // pipeline then minifies + fingerprints + rewrites references to.
    //
    // The hook's redirect is relative, so this also pins where hooks run: the
    // configured project root, not the process cwd. Driven in-process it fails
    // outright if that regresses.
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nhooks {\n  before \"mkdir -p assets && printf 'body{color:red}' > assets/gen.css\"\n}\nassets {\n  fingerprint #true\n}\n",
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/gen.css\"))",
    );
    site.stats();
    let names = site.files("public/assets");
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("gen.") && has_ext(n, "css")),
        "hook output reached the asset pipeline: {names:?}"
    );
    // Fingerprinting is a `css` capability, so only that flavor renames the file
    // and rewrites the reference; a slim build serves the generated name as-is.
    // The assertions above hold either way, which is what pins where hooks run.
    #[cfg(feature = "css")]
    {
        let html = fs::read_to_string(site.root.join("public/index.html")).unwrap();
        assert!(
            !html.contains("href=\"/assets/gen.css\""),
            "reference rewritten: {html}"
        );
    }
}

/// Assets are fingerprinted before pages compile, so a page failing to compile
/// used to abort *after* the asset tree had been regenerated: `dist` kept the
/// previous build's HTML pointing at asset filenames that no longer existed, and
/// every stylesheet 404'd. A failed build must leave `dist` exactly as it was.
#[test]
fn a_failed_build_leaves_the_previous_assets_in_place() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  fingerprint #true\n}\n",
    );
    site.write("assets/app.css", "body { color: red }\n");
    site.write("templates/broken.typ", "#let broken(page, body) = body\n");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\", template: \"broken.typ\",)\nhome",
    );
    site.stats();
    let served = site.files("public/assets");
    assert_eq!(served.len(), 1, "{served:?}");
    let html = site.output("index.html");
    assert!(
        html.contains(&served[0]) || !html.contains("app."),
        "{html}"
    );

    // Edit the stylesheet (so a regenerated tree would be named differently)
    // and break the layout in the same build. The break has to be one that
    // survives discovery and fails at compile, which is where the window is.
    site.write("assets/app.css", "body { color: blue }\n");
    site.write("templates/broken.typ", "#let broken(page, body) = #(");
    site.build_error();
    assert_eq!(
        site.files("public/assets"),
        served,
        "a failed build replaced the assets the existing HTML references"
    );
}

/// The broken-link check must not weaken on rebuild. Feeding it only
/// freshly-compiled pages meant a second, fully-cached build reported nothing,
/// and with `links { strict #true }` the gate passed outright.
#[test]
fn broken_links_are_still_reported_on_a_cached_rebuild() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nlinks { strict #false }\n",
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\n#link(\"missing.typ\")[gone]",
    );

    let first = site.run(&["build"]);
    let logs = String::from_utf8_lossy(&first.stderr).into_owned();
    assert!(logs.contains("missing.typ"), "first build: {logs}");

    let second = site.run(&["build", "-v"]);
    let logs = String::from_utf8_lossy(&second.stderr).into_owned();
    assert!(logs.contains("1 cached"), "rebuild was not cached: {logs}");
    assert!(
        logs.contains("missing.typ"),
        "the broken-link check went quiet on a cached rebuild: {logs}"
    );
}
