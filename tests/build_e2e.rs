//! The end-to-end builds that a scenario cannot express: they need the CLI's
//! own stderr, the cache's internal files, or the library's own return values.

mod common;

use std::fs;

use baudelaire::content::Discovery;

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
    assert!(stderr.contains("bad.typ"), "no source named: {stderr}");
    assert!(stderr.contains("invalid_func"), "no span excerpt: {stderr}");
    assert!(stderr.contains("bad.typ:2"), "no line number: {stderr}");
}

/// A refusal about `source` underlines the key that caused it.
#[cfg(feature = "markdown")]
#[test]
fn a_source_refusal_underlines_the_key() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
                sources { changelog "notes/a.md" }
            }
        "#,
    );
    site.write("notes/a.md", "A.\n");
    site.write(
        "content/page.md",
        ";;;\ntitle \"P\"\ndescription \"x\"\nsource \"changlog\"\n;;;\n",
    );

    let out = site.run(&["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown_source"), "{stderr}");
    assert!(stderr.contains("page.md:4"), "no line number: {stderr}");
    assert!(
        stderr.contains("no declaration under this name"),
        "no label: {stderr}"
    );
}

/// ...and the refusal for a source of a kind nothing reads underlines the page
/// too, rather than the file the key names.
#[cfg(feature = "markdown")]
#[test]
fn an_unreadable_source_underlines_the_page_that_named_it() {
    let site = Site::with(
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
                sources { notes "notes/n.rst" }
            }
        "#,
    );
    site.write("notes/n.rst", "notes\n");
    site.write("content/p.md", "---\ntitle: P\nsource: notes\n---\n");

    let out = site.run(&["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("source_unreadable"), "{stderr}");
    assert!(stderr.contains("p.md:3"), "no page label: {stderr}");
    assert!(
        !stderr.contains("[notes/n.rst"),
        "labelled with the declared file: {stderr}"
    );
}

/// A declared source the build cannot read says why, rather than answering
/// with typst's `NotFound` about a path the author can see.
#[test]
fn an_unreadable_declared_source_reports_the_real_reason() {
    let site = Site::with(
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
                sources { notes "shared/adir.typ" }
            }
        "#,
    );
    // A directory where a file was declared: the one unreadable case that is
    // portable to set up.
    site.write("shared/adir.typ/keep.txt", "x\n");
    site.write(
        "content/p.typ",
        "#import \"@baudelaire/sources:0.1.0\": notes\n\
         #let frontmatter = (title: \"P\",)\n\
         #include notes\n",
    );

    let out = site.run(&["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("directory"),
        "reported something other than the real reason: {stderr}"
    );
    assert!(
        !stderr.contains("file not found"),
        "still reporting not-found: {stderr}"
    );
}

#[test]
fn a_theme_set_under_class_highlighting_warns_that_it_is_ignored() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            html { highlight }
            paths {
                content "content"
                dist "public"
            }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\n\
         #show raw: set raw(theme: \"/palette.tmTheme\")\n\
         ```rust\nlet x = 1;\n```\n",
    );
    site.write("palette.tmTheme", THEME);

    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "a warning, not a failure: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("theme is ignored"), "no warning: {stderr}");
    assert!(stderr.contains("highlight #false"), "no way out: {stderr}");
    assert_eq!(
        stderr.matches("theme is ignored").count(),
        1,
        "warned more than once: {stderr}"
    );
    assert!(site.read("public/index.html").contains("sx-keyword"));
}

/// A `.tmTheme` with one rule, which is all that has to parse for typst to
/// accept it as a theme.
const THEME: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>name</key>
  <string>One rule</string>
  <key>settings</key>
  <array>
    <dict>
      <key>scope</key>
      <string>keyword</string>
      <key>settings</key>
      <dict>
        <key>foreground</key>
        <string>#ff0000</string>
      </dict>
    </dict>
  </array>
</dict>
</plist>
"#;

#[test]
fn error_in_a_bound_template_renders_against_the_template_file() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\ncontent {\n  collections { pages { template \"page.typ\" } }\n}\n",
    );
    // Padding pushes the erroring span past the length of the page source.
    site.write(
        "templates/page.typ",
        "#let pad = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n#let page(meta, body) = html.elem(\"body\", { body; nope_undefined })\n",
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
    // The summary is CLI output, so this one runs the real binary.
    let logs = site.build();
    assert!(logs.contains("1 asset"), "assets counted: {logs}");
    assert!(logs.contains("file"), "generated files counted: {logs}");
    assert!(logs.contains("╰─ public"), "output dir shown: {logs}");
}

/// The orphan report names the page nothing points at, and not the one every
/// layout points at.
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
    // Named as an *orphan*: every page also appears in the progress lines.
    let orphaned = |page: &str| logs.contains(&format!("`{page}` is linked from nowhere"));
    assert!(orphaned("lonely.typ"), "{logs}");
    assert!(
        !orphaned("seen.typ"),
        "a page linked from content is not an orphan: {logs}"
    );
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
    // The `.jpg` extension has to match the `jpeg` format leniently.
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
    let cols = Discovery::all(&site.config(), &project(&site.config())).unwrap();
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
    let manifest = fs::read_to_string(site.root.join(".baudelaire/cache/manifest.json")).unwrap();
    assert!(
        !manifest.contains("Distinct Body Text"),
        "no html in manifest: {manifest}"
    );
    assert!(
        manifest.contains("blob"),
        "manifest points at blobs: {manifest}"
    );
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
    // The hook's redirect is relative, so this also pins that hooks run in the
    // configured project root and not the process cwd.
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
    // Fingerprinting is a `css` capability, so only that flavor rewrites the
    // reference; a slim build serves the generated name as-is.
    #[cfg(feature = "css")]
    {
        let html = fs::read_to_string(site.root.join("public/index.html")).unwrap();
        assert!(
            !html.contains("href=\"/assets/gen.css\""),
            "reference rewritten: {html}"
        );
    }
}

/// Assets are fingerprinted before pages compile, so a failed build has to
/// leave `dist` exactly as it was.
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

    // The stylesheet is edited so a regenerated tree would be named
    // differently, and the break has to survive discovery and fail at compile.
    site.write("assets/app.css", "body { color: blue }\n");
    site.write("templates/broken.typ", "#let broken(page, body) = #(");
    site.build_error();
    assert_eq!(
        site.files("public/assets"),
        served,
        "a failed build replaced the assets the existing HTML references"
    );
}

/// The broken-link check must not weaken on rebuild: fed only freshly-compiled
/// pages, a fully-cached build reports nothing.
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

/// A template nothing supplies is one diagnostic naming what asked for it,
/// raised before the first compile.
#[test]
fn a_missing_template_names_what_asked_for_it() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
                templates "templates"
            }
            content {
                collections {
                    _root { template "layout.typ" }
                }
            }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nhi",
    );
    let out = site.run(&["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("baudelaire::template::missing"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("layout.typ"), "stderr: {stderr}");
    assert!(
        stderr.contains("content/index.typ"),
        "names the page that asked: {stderr}"
    );
    assert!(
        !stderr.contains("file not found"),
        "raw typst report leaked: {stderr}"
    );
}
