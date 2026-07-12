mod common;

use std::fs;

use baudelaire::content::discover;
use common::{CONFIG, Site};

#[test]
fn builds_simple_page_to_html() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        r#"#frontmatter((title: "Hello",))
Hello, world!
"#,
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let html = fs::read_to_string(site.root.join("public/posts/hello/index.html")).unwrap();
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Hello, world!"));
}

#[test]
fn builds_multiple_pages_in_parallel() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
        "#,
    );
    for i in 0..8 {
        site.write(
            &format!("content/posts/p{i}.typ"),
            &format!("#frontmatter((title: \"P{i}\",))\nPage {i}"),
        );
    }
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for i in 0..8 {
        let path = site.root.join(format!("public/posts/p{i}/index.html"));
        assert!(path.exists(), "missing output for p{i}");
        let html = fs::read_to_string(path).unwrap();
        assert!(html.contains(&format!("Page {i}")));
    }
}

#[test]
fn flat_urls_produce_html_files() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #false
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        "#frontmatter((title: \"Hi\",))\nbody text",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let path = site.root.join("public/posts/hello.html");
    assert!(path.exists());
}

#[test]
fn drafts_skipped_by_default() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
        "#,
    );
    site.write(
        "content/posts/draft.typ",
        "#frontmatter((title: \"D\", draft: true,))\ndraft body text",
    );
    site.write(
        "content/posts/real.typ",
        "#frontmatter((title: \"R\",))\nreal body text",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!site.root.join("public/posts/draft/index.html").exists());
    assert!(site.root.join("public/posts/real/index.html").exists());
}

#[test]
fn drafts_flag_builds_drafts() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
        "#,
    );
    site.write(
        "content/posts/draft.typ",
        "#frontmatter((title: \"D\", draft: true,))\ndraft body text",
    );
    let out = site.run(&["--drafts", "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(site.root.join("public/posts/draft/index.html").exists());
}

#[test]
fn draft_suffix_marks_and_strips_slug() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
        "#,
    );
    site.write(
        "content/posts/wip.draft.typ",
        "#frontmatter((title: \"W\",))\nwork in progress",
    );
    // Skipped by default (suffix implies draft)...
    assert!(site.run(&["build"]).status.success());
    assert!(!site.root.join("public/posts/wip/index.html").exists());
    // ...but built with --drafts, and the `.draft` suffix is stripped from the slug.
    assert!(site.run(&["--drafts", "build"]).status.success());
    assert!(site.root.join("public/posts/wip/index.html").exists());
}

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
        "#frontmatter((title: \"OK\",))\nfine",
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
    assert!(stderr.contains("typst") || stderr.contains("error"));
}

#[test]
fn error_in_a_bound_template_renders_against_the_template_file() {
    // A span reaching into another file (here a template, whose text differs in
    // length from the page's) must resolve against that file — never overrun the
    // page source and crash the renderer with an `OutOfBounds` panic.
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\ncollections { pages template=\"page.typ\" }\n",
    );
    // Padding pushes the erroring span past the length of the short page source.
    site.write(
        "templates/page.typ",
        "#let pad = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n#let page(meta, body) = html.elem(\"html\", html.elem(\"body\", { body; nope_undefined }))\n",
    );
    site.write("content/pages/a.typ", "#frontmatter((title: \"A\",))\nhi");
    let out = site.run(&["build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("OutOfBounds"), "renderer panicked: {stderr}");
    assert!(stderr.contains("unknown variable"), "{stderr}");
    // The label lands in the template, not the page.
    assert!(stderr.contains("page.typ"), "points at the template: {stderr}");
}

#[test]
fn build_summary_reports_assets_generated_files_and_output_dir() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://x.example\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\noutput {\n  search { formats \"json\" }\n}\n",
    );
    site.write("assets/style.css", "body { color: red; }");
    site.write("content/a.typ", "#frontmatter((title: \"A\",))\nbody");
    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The compact summary line counts assets and generated files, and shows dist.
    assert!(stdout.contains("1 asset"), "assets counted: {stdout}");
    assert!(stdout.contains("file"), "generated files counted: {stdout}");
    assert!(stdout.contains("→ public"), "output dir shown: {stdout}");
}

#[test]
fn meta_tags_injected_from_frontmatter_and_config() {
    let site = Site::new();
    site.write("config.kdl", "site \"S\"\nurl \"https://s.example\"\n");
    site.write(
        "content/post.typ",
        "#frontmatter((title: \"Hello\", summary: \"A short summary.\"))\nbody",
    );
    assert!(site.run(&["build"]).status.success());
    let html = fs::read_to_string(site.root.join("public/post/index.html")).unwrap();
    assert!(html.contains("name=\"description\" content=\"A short summary.\""), "{html}");
    assert!(html.contains("property=\"og:title\" content=\"Hello\""), "{html}");
    assert!(html.contains("property=\"og:site_name\" content=\"S\""), "{html}");
    // URL-absolute tags appear because a base `url` is set.
    assert!(html.contains("rel=\"canonical\" href=\"https://s.example/post/\""), "{html}");
    assert!(html.contains("property=\"og:url\" content=\"https://s.example/post/\""), "{html}");
}

#[test]
fn og_image_is_fingerprinted_and_absolute() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"S\"\nurl \"https://s.example\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\noutput {\n  assets { fingerprint #true }\n}\n",
    );
    site.write_bytes("assets/pic.png", include_bytes!("fixtures/bloated.png"));
    site.write(
        "content/post.typ",
        "#frontmatter((title: \"Hi\", image: \"/assets/pic.png\"))\nbody",
    );
    assert!(site.run(&["build"]).status.success());
    let html = fs::read_to_string(site.root.join("public/post/index.html")).unwrap();
    assert!(html.contains("property=\"og:image\""), "{html}");
    // Absolute, and pointing at the content-hashed filename (not the raw name).
    assert!(html.contains("content=\"https://s.example/assets/pic."), "absolute + hashed: {html}");
    assert!(!html.contains("assets/pic.png\""), "raw name replaced by hash: {html}");
}

#[test]
fn meta_tags_omitted_when_disabled() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"S\"\nurl \"https://s.example\"\noutput {\n  html { meta #false }\n}\n",
    );
    site.write("content/post.typ", "#frontmatter((title: \"Hi\", summary: \"s\"))\nbody");
    assert!(site.run(&["build"]).status.success());
    let html = fs::read_to_string(site.root.join("public/post/index.html")).unwrap();
    assert!(!html.contains("og:title"), "meta disabled: {html}");
}

#[test]
fn optimize_losslessly_shrinks_png_assets() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\noutput {\n  images { optimize { png } }\n}\n",
    );
    site.write("content/a.typ", "#frontmatter((title: \"A\",))\nbody");
    // A PNG bloated with strippable metadata and a stored (uncompressed) IDAT.
    let png = include_bytes!("fixtures/bloated.png");
    site.write_bytes("assets/pic.png", png);
    assert!(site.run(&["build"]).status.success());

    let out = fs::read(site.root.join("public/assets/pic.png")).unwrap();
    assert!(out.len() < png.len(), "optimized {} < original {}", out.len(), png.len());
    // Still a valid PNG (signature intact).
    assert_eq!(&out[..8], b"\x89PNG\r\n\x1a\n", "output is a PNG");
}

#[test]
fn optimize_reencodes_jpeg_with_lax_extension() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\noutput {\n  images { optimize { jpeg quality=70 } }\n}\n",
    );
    site.write("content/a.typ", "#frontmatter((title: \"A\",))\nbody");
    // A high-quality JPEG shrinks when re-encoded at quality 70. The `.jpg`
    // extension must match the `jpeg` format leniently.
    let jpg = include_bytes!("fixtures/big.jpg");
    site.write_bytes("assets/photo.jpg", jpg);
    assert!(site.run(&["build"]).status.success());

    let out = fs::read(site.root.join("public/assets/photo.jpg")).unwrap();
    assert!(out.len() < jpg.len(), "re-encoded {} < original {}", out.len(), jpg.len());
    assert_eq!(&out[..2], b"\xff\xd8", "output is a JPEG");
}

#[test]
fn images_get_lazy_loading_attributes() {
    let site = Site::new();
    site.write("config.kdl", "site \"S\"\n");
    site.write(
        "content/p.typ",
        "#frontmatter((title: \"P\"))\n#html.elem(\"img\", attrs: (src: \"/photo.png\"))",
    );
    assert!(site.run(&["build"]).status.success());
    let html = fs::read_to_string(site.root.join("public/p/index.html")).unwrap();
    assert!(html.contains("loading=\"lazy\""), "{html}");
    assert!(html.contains("decoding=\"async\""), "{html}");
}

#[test]
fn copies_assets_to_dist() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
                assets "assets"
            }
        "#,
    );
    site.write(
        "content/posts/p.typ",
        "#frontmatter((title: \"P\",))\nbody text",
    );
    site.write("assets/style.css", "body { color: red; }");
    site.write("assets/img/logo.png", "fake-png");
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(site.root.join("public/assets/style.css").exists());
    assert!(site.root.join("public/assets/img/logo.png").exists());
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
            clean #true
        "#,
    );
    site.write(
        "content/posts/2024/jan.typ",
        "#frontmatter((title: \"Jan\",))\nJanuary",
    );
    site.write(
        "content/posts/2024/feb.typ",
        "#frontmatter((title: \"Feb\",))\nFebruary",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cols = discover(&site.config()).unwrap();
    let posts = cols.iter().find(|c| c.id == "posts").unwrap();
    assert_eq!(posts.pages.len(), 2);
}

#[test]
fn custom_permalink_template_output() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
            collections {
              posts permalink="/blog/{slug}/"
            }
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        "#frontmatter((title: \"Hi\",))\nbody text",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(site.root.join("public/blog/hello/index.html").exists());
}

#[test]
fn internal_typ_links_rewritten_to_permalinks() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
        "#,
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\",))\nSee #link(\"b.typ\")[B] and #link(\"https://example.com\")[ext].",
    );
    site.write("content/posts/b.typ", "#frontmatter((title: \"B\",))\nB body");
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let html = fs::read_to_string(site.root.join("public/posts/a/index.html")).unwrap();
    assert!(
        html.contains("href=\"/posts/b/\""),
        "internal link not rewritten: {html}"
    );
    assert!(
        html.contains("https://example.com"),
        "external link should be preserved: {html}"
    );
}

#[test]
fn broken_internal_link_fails_strict_build() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\",))\nSee #link(\"missing.typ\")[gone].",
    );
    let out = site.run(&["build"]);
    assert!(!out.status.success(), "strict build should fail on broken link");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("broken internal link"), "stderr: {stderr}");
}

#[test]
fn broken_internal_link_warns_when_not_strict() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\",))\nSee #link(\"missing.typ\")[gone].",
    );
    let out = site.run(&["--strict-links", "false", "build"]);
    assert!(
        out.status.success(),
        "non-strict build should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(site.root.join("public/posts/a/index.html").exists());
}

#[test]
fn layout_template_wraps_page_body() {
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
            clean #true
        "#,
    );
    site.write(
        "templates/post.typ",
        "#let post(page, body) = {\n  html.elem(\"article\", attrs: (class: \"post\"))[\n    #html.elem(\"h1\", page.frontmatter.title)\n    #body\n  ]\n}\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#frontmatter((title: \"Hi\", template: \"post.typ\",))\nHello body",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let html = fs::read_to_string(site.root.join("public/posts/hello/index.html")).unwrap();
    assert!(html.contains("<article class=\"post\">"), "layout not applied: {html}");
    // The heading carries an anchor id (anchors default on); assert the title
    // text reached the h1 without pinning the exact attributes.
    assert!(html.contains(">Hi</h1>"), "frontmatter data not passed: {html}");
    assert!(html.contains("Hello body"), "body not embedded: {html}");
}

#[test]
fn layout_template_default_from_collection() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            collections {
              posts template="post.typ"
            }
        "#,
    );
    site.write(
        "templates/post.typ",
        "#let post(page, body) = html.elem(\"main\", body)\n",
    );
    // No `template` in frontmatter — inherited from the collection default.
    site.write(
        "content/posts/hello.typ",
        "#frontmatter((title: \"Hi\",))\nbody here",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let html = fs::read_to_string(site.root.join("public/posts/hello/index.html")).unwrap();
    assert!(html.contains("<main>"), "collection default layout not applied: {html}");
}

#[test]
fn frontmatter_redirects_emit_stub_pages() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write(
        "content/posts/new.typ",
        "#frontmatter((title: \"New\", slug: \"new\", redirect: (\"/old\", \"/legacy/post\"),))\nbody",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for old in ["public/old/index.html", "public/legacy/post/index.html"] {
        let stub = fs::read_to_string(site.root.join(old)).unwrap();
        assert!(stub.contains("http-equiv=\"refresh\""), "not a redirect: {stub}");
        assert!(stub.contains("url=/posts/new/"), "wrong target: {stub}");
    }
    // The real page is still generated.
    assert!(site.root.join("public/posts/new/index.html").exists());
}

#[test]
fn taxonomy_index_and_term_pages_generated() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "T"
            paths {
                content "content"
                dist "public"
            }
            clean #true
            taxonomies {
              tags index=#true
            }
        "#,
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"Alpha\", slug: \"a\", tags: (\"intro\", \"rust\"),))\na",
    );
    site.write(
        "content/posts/b.typ",
        "#frontmatter((title: \"Beta\", slug: \"b\", tags: (\"intro\",),))\nb",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Index lists both terms, linking to their term pages.
    let index = fs::read_to_string(site.root.join("public/tags/index.html")).unwrap();
    assert!(index.contains("href=\"/tags/intro/\""), "index: {index}");
    assert!(index.contains("href=\"/tags/rust/\""), "index: {index}");
    // The `intro` term page links to both members.
    let intro = fs::read_to_string(site.root.join("public/tags/intro/index.html")).unwrap();
    assert!(intro.contains("href=\"/posts/a/\""), "term page: {intro}");
    assert!(intro.contains("href=\"/posts/b/\""), "term page: {intro}");
    assert!(intro.contains("Alpha") && intro.contains("Beta"), "term page: {intro}");
    // `rust` has a single member.
    assert!(site.root.join("public/tags/rust/index.html").exists());
}

#[test]
fn pagination_splits_collection_into_index_pages() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "T"
            paths {
                content "content"
                dist "public"
            }
            clean #true
            collections {
              posts sort="title" paginate=2
            }
        "#,
    );
    for (i, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        site.write(
            &format!("content/posts/{name}.typ"),
            &format!("#frontmatter((title: \"P{i}\", slug: \"{name}\",))\nbody"),
        );
    }
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // 5 items / 2 per page → 3 index pages.
    let p1 = fs::read_to_string(site.root.join("public/posts/index.html")).unwrap();
    assert!(p1.contains("href=\"/posts/page/2/\""), "page 1 needs next: {p1}");
    assert!(!p1.contains("← Previous"), "page 1 has no prev: {p1}");
    assert!(site.root.join("public/posts/page/2/index.html").exists());
    let p3 = fs::read_to_string(site.root.join("public/posts/page/3/index.html")).unwrap();
    assert!(p3.contains("href=\"/posts/page/2/\""), "page 3 links back: {p3}");
    assert!(!p3.contains("Next →"), "last page has no next: {p3}");
}

#[test]
fn sitemap_and_rss_emitted_when_url_set() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://example.com\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\", slug: \"a\", date: datetime(year: 2024, month: 1, day: 2),))\nbody",
    );
    assert!(site.run(&["build"]).status.success());
    let sitemap = fs::read_to_string(site.root.join("public/sitemap.xml")).unwrap();
    assert!(sitemap.contains("<loc>https://example.com/posts/a/</loc>"), "{sitemap}");
    let rss = fs::read_to_string(site.root.join("public/rss.xml")).unwrap();
    assert!(rss.contains("<item>"), "{rss}");
    assert!(rss.contains("https://example.com/posts/a/"), "{rss}");
    assert!(rss.contains("<pubDate>"), "{rss}");
}

#[test]
fn atom_feed_when_configured() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://example.com\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\noutput {\n  feed {\n    formats \"atom\"\n  }\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\", slug: \"a\", date: datetime(year: 2024, month: 1, day: 2),))\nbody",
    );
    assert!(site.run(&["build"]).status.success());
    let atom = fs::read_to_string(site.root.join("public/atom.xml")).unwrap();
    assert!(atom.contains("http://www.w3.org/2005/Atom"), "{atom}");
    assert!(atom.contains("<entry>"), "{atom}");
    // Only the configured format is emitted.
    assert!(!site.root.join("public/rss.xml").exists());
}

#[test]
fn search_indexes_emitted_for_each_configured_format() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\ntaxonomies {\n  tags\n}\noutput {\n  search {\n    formats \"json\" \"inverted\"\n    stopwords \"the\"\n    client #true\n  }\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"Alpha\", slug: \"a\", tags: (\"intro\",),))\nThe quick brown fox",
    );
    assert!(
        site.run(&["build"]).status.success(),
        "build failed",
    );

    let docs = fs::read_to_string(site.root.join("public/search.json")).unwrap();
    assert!(docs.contains("\"url\":\"/posts/a/\""), "{docs}");
    assert!(docs.contains("\"title\":\"Alpha\""), "{docs}");
    assert!(docs.contains("quick brown fox"), "body indexed: {docs}");
    assert!(docs.contains("\"intro\""), "tags indexed: {docs}");

    let index = fs::read_to_string(site.root.join("public/search.inverted.json")).unwrap();
    assert!(index.contains("\"postings\""), "{index}");
    assert!(index.contains("\"quick\""), "term indexed: {index}");
    // Stopword excluded from the inverted index.
    assert!(!index.contains("\"the\""), "stopword dropped: {index}");

    // `client #true` emits a self-mounting command palette per format: the
    // engine (`createSearch`), the palette UI (`mountSearch`), and an auto-mount
    // so one `<script>` is a working search box.
    let client = fs::read_to_string(site.root.join("public/search.js")).unwrap();
    assert!(client.contains("export async function createSearch"), "{client}");
    assert!(client.contains("export function mountSearch"), "{client}");
    assert!(client.trim_end().ends_with("mountSearch();"), "auto-mounts: {client}");
    assert!(
        site.root.join("public/search.inverted.js").exists(),
        "inverted client emitted",
    );
}

#[test]
fn search_client_bundles_into_a_user_entry_via_virtual_module() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  assets { bundle #true }\n  search { formats \"json\" }\n}\n",
    );
    site.write(
        "assets/main.js",
        "import { mountSearch } from \"baudelaire:search\";\nmountSearch({ hotkey: \"k\" });\n",
    );
    site.write("content/a.typ", "#frontmatter((title: \"A\",))\nbody");
    assert!(
        site.run(&["build"]).status.success(),
        "build failed",
    );
    // rolldown resolves the virtual specifier and inlines the palette into the
    // user's own bundle (no separate fetch of a generated file).
    let bundle = fs::read_to_string(site.root.join("public/assets/main.js")).unwrap();
    assert!(bundle.contains("bd-palette"), "palette inlined: {bundle}");
    assert!(bundle.contains("search.json"), "engine inlined: {bundle}");
    // The bare import was resolved and inlined, not left for the browser to fetch.
    assert!(!bundle.contains("import {"), "specifier resolved away: {bundle}");
}

#[test]
fn no_search_index_without_configured_formats() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nbody");
    assert!(site.run(&["build"]).status.success());
    assert!(!site.root.join("public/search.json").exists());
    assert!(!site.root.join("public/search-index.json").exists());
}

#[test]
fn embed_inlines_local_assets_as_data_uris() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  html {\n    embed #true\n  }\n}\n",
    );
    site.write("assets/style.css", "body{color:red}");
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\",))\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/style.css\"))",
    );
    assert!(
        site.run(&["build"]).status.success(),
        "build failed",
    );
    let html = fs::read_to_string(site.root.join("public/posts/a/index.html")).unwrap();
    // `body{color:red}` base64-encodes to this; the /assets ref is inlined.
    assert!(html.contains("data:text/css;base64,"), "{html}");
    assert!(!html.contains("href=\"/assets/style.css\""), "ref replaced: {html}");
}

#[test]
fn embed_inlines_processed_not_source_bytes() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  html { embed #true }\n  assets { minify #true }\n}\n",
    );
    // A comment survives only in the raw source; minification drops it.
    site.write("assets/style.css", "/* source-only comment */\nbody {\n  color: red;\n}\n");
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\",))\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/style.css\"))",
    );
    assert!(site.run(&["build"]).status.success(), "build failed");
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
    assert!(!decoded.contains("/*"), "inlined raw source, not processed: {decoded}");
    assert!(decoded.contains("red"), "declaration lost: {decoded}");
}

#[test]
fn robots_txt_emitted_when_block_present() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://example.com\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\noutput {\n  robots {\n    disallow \"/private/\"\n  }\n}\n",
    );
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nbody");
    assert!(site.run(&["build"]).status.success());
    let robots = fs::read_to_string(site.root.join("public/robots.txt")).unwrap();
    assert!(robots.contains("User-agent: *"), "{robots}");
    assert!(robots.contains("Disallow: /private/"), "{robots}");
    assert!(robots.contains("Sitemap: https://example.com/sitemap.xml"), "{robots}");
}

#[test]
fn no_robots_txt_without_block() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nbody");
    assert!(site.run(&["build"]).status.success());
    assert!(!site.root.join("public/robots.txt").exists());
}

#[test]
fn build_context_exposed_via_sys_inputs() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write(
        "content/posts/v.typ",
        "#frontmatter((title: \"V\", slug: \"v\",))\nversion=#sys.inputs.baudelaire.version site=#sys.inputs.baudelaire.site.title mode=#sys.inputs.baudelaire.mode",
    );
    assert!(site.run(&["build"]).status.success());
    let html = fs::read_to_string(site.root.join("public/posts/v/index.html")).unwrap();
    assert!(
        html.contains(&format!("version={}", env!("CARGO_PKG_VERSION"))),
        "{html}"
    );
    assert!(html.contains("site=T"), "site mirror exposed: {html}");
    assert!(html.contains("mode=build"), "build mode exposed: {html}");
}

#[test]
fn llms_txt_indexes_pages_by_collection() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://example.com\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\noutput {\n  llms {\n    summary \"A test site.\"\n  }\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"Alpha\", slug: \"a\",))\nbody",
    );
    assert!(site.run(&["build"]).status.success());
    let llms = fs::read_to_string(site.root.join("public/llms.txt")).unwrap();
    assert!(llms.contains("# T"), "{llms}");
    assert!(llms.contains("> A test site."), "{llms}");
    assert!(llms.contains("## posts"), "{llms}");
    assert!(llms.contains("- [Alpha](https://example.com/posts/a/)"), "{llms}");
}

#[test]
fn no_feed_or_sitemap_without_url() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\", date: datetime(year: 2024, month: 1, day: 2),))\nbody",
    );
    assert!(site.run(&["build"]).status.success());
    assert!(!site.root.join("public/sitemap.xml").exists());
    assert!(!site.root.join("public/rss.xml").exists());
}

#[test]
fn taxonomy_listing_uses_custom_template() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\ntaxonomies {\n  tags index=#true template=\"tag.typ\"\n}\n",
    );
    // The template controls the page from the structured listing data.
    site.write(
        "templates/tag.typ",
        "#let tag(page, body) = html.elem(\"main\", attrs: (class: \"tax\"))[\n  #html.elem(\"h2\", page.frontmatter.title)\n  #for e in page.frontmatter.entries [ #html.elem(\"a\", attrs: (href: e.url), e.label) ]\n]\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"Alpha\", slug: \"a\", tags: (\"intro\",),))\nbody",
    );
    assert!(site.run(&["build"]).status.success());
    let html = fs::read_to_string(site.root.join("public/tags/intro/index.html")).unwrap();
    assert!(html.contains("<main class=\"tax\">"), "template not applied: {html}");
    assert!(html.contains(">Tags: intro</h2>"), "title data missing: {html}");
    assert!(html.contains("href=\"/posts/a/\""), "entries data missing: {html}");
    assert!(html.contains("Alpha"), "entry label missing: {html}");
}

#[test]
fn empty_site_builds_without_error() {
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
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn fingerprint_renames_assets_and_rewrites_references() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  assets {\n    fingerprint #true\n  }\n}\n",
    );
    site.write("assets/style.css", "body{color:red}");
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\",))\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/style.css\"))",
    );
    assert!(
        site.run(&["build"]).status.success(),
        "build failed"
    );
    // The original name is gone; a content-hashed one replaces it.
    let names = site.files("public/assets");
    assert!(!names.iter().any(|n| n == "style.css"), "{names:?}");
    let hashed = names
        .iter()
        .find(|n| n.starts_with("style.") && n.ends_with(".css"))
        .expect("a fingerprinted stylesheet");
    // The page reference is rewritten to the hashed URL.
    let html = fs::read_to_string(site.root.join("public/posts/a/index.html")).unwrap();
    assert!(html.contains(&format!("/assets/{hashed}")), "{html}");
    assert!(!html.contains("href=\"/assets/style.css\""), "{html}");
}

#[test]
fn css_url_references_are_fingerprinted() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  assets { fingerprint #true }\n}\n",
    );
    site.write("assets/style.css", "body{background:url(bg.png)}");
    site.write("assets/bg.png", "PNGDATA");
    site.write("content/a.typ", "#frontmatter((title: \"A\",))\nbody");
    assert!(site.run(&["build"]).status.success(), "build failed");
    let names = site.files("public/assets");
    let bg = names
        .iter()
        .find(|n| n.starts_with("bg.") && n.ends_with(".png"))
        .expect("a fingerprinted image");
    let css_name = names
        .iter()
        .find(|n| n.starts_with("style.") && n.ends_with(".css"))
        .expect("a fingerprinted stylesheet");
    let css = fs::read_to_string(site.root.join(format!("public/assets/{css_name}"))).unwrap();
    // The `url()` now points at the image's hashed name, not the original.
    assert!(css.contains(&format!("/assets/{bg}")), "url() not rewritten: {css}");
    assert!(!css.contains("bg.png\"") && !css.contains("(bg.png)"), "stale url(): {css}");
}

#[test]
fn srcset_urls_are_fingerprinted() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  assets { fingerprint #true }\n}\n",
    );
    site.write("assets/a.png", "AAA");
    site.write("assets/b.png", "BBB");
    site.write(
        "content/a.typ",
        "#frontmatter((title: \"A\",))\n#html.elem(\"img\", attrs: (srcset: \"/assets/a.png 1x, /assets/b.png 2x\"))",
    );
    assert!(site.run(&["build"]).status.success(), "build failed");
    let names = site.files("public/assets");
    let a = names.iter().find(|n| n.starts_with("a.") && n.ends_with(".png")).expect("hashed a");
    let b = names.iter().find(|n| n.starts_with("b.") && n.ends_with(".png")).expect("hashed b");
    let html = fs::read_to_string(site.root.join("public/a/index.html")).unwrap();
    // Each candidate URL is fingerprinted; its descriptor is preserved.
    assert!(html.contains(&format!("/assets/{a} 1x")), "srcset a not rewritten: {html}");
    assert!(html.contains(&format!("/assets/{b} 2x")), "srcset b not rewritten: {html}");
}

#[test]
fn minify_compacts_css() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  assets {\n    minify #true\n  }\n}\n",
    );
    site.write(
        "assets/style.css",
        "/* a comment */\nbody {\n  color: red;\n}\n",
    );
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nbody");
    assert!(site.run(&["build"]).status.success(), "build failed");
    let css = fs::read_to_string(site.root.join("public/assets/style.css")).unwrap();
    assert!(!css.contains("/*"), "comment stripped: {css}");
    assert!(!css.contains('\n'), "whitespace collapsed: {css}");
    assert!(css.contains("red"), "declaration kept: {css}");
}

#[test]
fn bundle_inlines_js_imports_and_drops_partials() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\noutput {\n  assets {\n    bundle #true\n  }\n}\n",
    );
    site.write("assets/_util.js", "export function hi() { return 42; }\n");
    site.write(
        "assets/main.js",
        "import { hi } from './_util.js';\nconsole.log(hi());\n",
    );
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\",))\nbody");
    assert!(site.run(&["build"]).status.success(), "build failed");
    let names = site.files("public/assets");
    assert!(names.iter().any(|n| n == "main.js"), "entry emitted: {names:?}");
    // The partial is pulled in through imports, never emitted standalone.
    assert!(!names.iter().any(|n| n == "_util.js"), "partial dropped: {names:?}");
    // The imported function body is bundled into the entry.
    let js = fs::read_to_string(site.root.join("public/assets/main.js")).unwrap();
    assert!(js.contains("42"), "import inlined: {js}");
}

#[test]
fn cache_stores_html_in_object_store_not_manifest() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"Unique Marker\",))\nDistinct Body Text",
    );
    assert!(site.run(&["build"]).status.success(), "build failed");
    // The manifest is metadata only — page markup lives in the object store.
    let manifest = fs::read_to_string(site.root.join(".baudelaire/cache/manifest.json")).unwrap();
    assert!(!manifest.contains("Distinct Body Text"), "no html in manifest: {manifest}");
    assert!(manifest.contains("blob"), "manifest points at blobs: {manifest}");
    // At least one content-addressed blob was written, holding the HTML.
    let objects = site.root.join(".baudelaire/cache/objects");
    assert!(objects.is_dir(), "object store created");
    let shard = fs::read_dir(&objects)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("a sharded object directory");
    let blob = fs::read_dir(&shard)
        .unwrap()
        .filter_map(|e| e.ok())
        .next()
        .expect("a blob file");
    let html = fs::read_to_string(blob.path()).unwrap();
    assert!(html.contains("Distinct Body Text"), "blob holds html: {html}");
}

#[test]
fn hooks_run_before_and_after_the_build() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nhooks {\n  before \"echo b > hook-before.txt\"\n  after \"echo a > hook-after.txt\"\n}\n",
    );
    site.write("content/index.typ", "#frontmatter((title: \"H\",))\nbody");
    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(site.root.join("hook-before.txt").exists(), "before hook ran");
    assert!(site.root.join("hook-after.txt").exists(), "after hook ran");
}

#[test]
fn a_failing_hook_fails_the_build() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nhooks {\n  before \"exit 7\"\n}\n",
    );
    site.write("content/index.typ", "#frontmatter((title: \"H\",))\nb");
    let out = site.run(&["build"]);
    assert!(!out.status.success(), "build should fail when a hook exits non-zero");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("hook"),
        "error should mention the hook: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn before_hook_output_flows_into_the_asset_pipeline() {
    // The Tailwind model: a before hook generates CSS into assets/, which the
    // pipeline then minifies + fingerprints + rewrites references to.
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nclean #true\nhooks {\n  before \"mkdir -p assets && printf 'body{color:red}' > assets/gen.css\"\n}\noutput {\n  assets { fingerprint #true }\n}\n",
    );
    site.write(
        "content/index.typ",
        "#frontmatter((title: \"H\",))\n#html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/gen.css\"))",
    );
    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let names = site.files("public/assets");
    assert!(
        names.iter().any(|n| n.starts_with("gen.") && n.ends_with(".css")),
        "hook-generated css was fingerprinted: {names:?}"
    );
    let html = fs::read_to_string(site.root.join("public/index.html")).unwrap();
    assert!(!html.contains("href=\"/assets/gen.css\""), "reference rewritten: {html}");
}

#[test]
fn duplicate_permalinks_are_rejected() {
    // Two pages with the same slug resolve to one URL — a silent overwrite
    // before, now a hard error naming both.
    let site = Site::new();
    site.write("config.kdl", CONFIG);
    site.write("content/posts/a.typ", "#frontmatter((title: \"A\", slug: \"same\",))\na");
    site.write("content/posts/b.typ", "#frontmatter((title: \"B\", slug: \"same\",))\nb");
    let out = site.run(&["build"]);
    assert!(!out.status.success(), "colliding permalinks must fail the build");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("resolve to") && err.contains("a.typ") && err.contains("b.typ"), "{err}");
}

#[test]
fn a_slug_with_no_url_safe_characters_is_rejected() {
    let site = Site::new();
    site.write("config.kdl", CONFIG);
    site.write("content/posts/p.typ", "#frontmatter((title: \"P\", slug: \"🎉\",))\np");
    let out = site.run(&["build"]);
    assert!(!out.status.success(), "an empty slug must fail the build");
    assert!(String::from_utf8_lossy(&out.stderr).contains("URL-safe"));
}

#[test]
fn colliding_taxonomy_terms_are_rejected() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n\
         taxonomies {\n  tags index=#true\n}\n",
    );
    site.write(
        "content/posts/a.typ",
        "#frontmatter((title: \"A\", tags: (\"C++\", \"C--\"),))\na",
    );
    let out = site.run(&["build"]);
    assert!(!out.status.success(), "two terms slugging to `c` must fail the build");
    assert!(String::from_utf8_lossy(&out.stderr).contains("slug to `c`"));
}

/// A configured `did` makes the build emit the standard.site verification
/// artifacts offline: the `.well-known` file and a per-page `<link>` on dated
/// pages. This is the whole build-time contract, exercised end to end.
#[test]
fn standard_verify_emits_wellknown_and_link_on_dated_pages() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths { content "content" dist "public" }
            clean #true
            publish {
                standard {
                    handle "me.example"
                    did "did:plc:test123"
                }
            }
        "#,
    );
    site.write(
        "content/posts/dated.typ",
        "#frontmatter((title: \"Dated\", date: datetime(year: 2026, month: 1, day: 2)))\nbody",
    );
    site.write("content/posts/undated.typ", "#frontmatter((title: \"Undated\",))\nbody");

    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let well = fs::read_to_string(site.root.join("public/.well-known/site.standard.publication")).unwrap();
    assert_eq!(well, "at://did:plc:test123/site.standard.publication/self");

    let dated = fs::read_to_string(site.root.join("public/posts/dated/index.html")).unwrap();
    assert!(dated.contains(r#"rel="site.standard.document""#), "{dated}");
    assert!(dated.contains("at://did:plc:test123/site.standard.document/"), "{dated}");

    // An undated page is not a document, so it carries no backlink.
    let undated = fs::read_to_string(site.root.join("public/posts/undated/index.html")).unwrap();
    assert!(!undated.contains("site.standard.document"), "{undated}");
}

/// Without a `did`, nothing standard.site-related touches the build output.
#[test]
fn standard_verify_absent_without_a_did() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths { content "content" dist "public" }
            clean #true
            publish { standard { handle "me.example" } }
        "#,
    );
    site.write(
        "content/posts/dated.typ",
        "#frontmatter((title: \"Dated\", date: datetime(year: 2026, month: 1, day: 2)))\nbody",
    );
    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!site.root.join("public/.well-known/site.standard.publication").exists());
    let html = fs::read_to_string(site.root.join("public/posts/dated/index.html")).unwrap();
    assert!(!html.contains("site.standard.document"), "{html}");
}

/// The verification toggles are honored: `verify { wellknown #false }` drops the
/// file while the per-page link stays.
#[test]
fn standard_verify_toggle_suppresses_wellknown_only() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths { content "content" dist "public" }
            clean #true
            publish {
                standard {
                    handle "me.example"
                    did "did:plc:test123"
                    verify { wellknown #false }
                }
            }
        "#,
    );
    site.write(
        "content/posts/dated.typ",
        "#frontmatter((title: \"Dated\", date: datetime(year: 2026, month: 1, day: 2)))\nbody",
    );
    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!site.root.join("public/.well-known/site.standard.publication").exists());
    let html = fs::read_to_string(site.root.join("public/posts/dated/index.html")).unwrap();
    assert!(html.contains(r#"rel="site.standard.document""#), "{html}");
}

#[test]
fn headings_get_unique_slug_anchors() {
    let site = Site::new();
    site.write("config.kdl", CONFIG);
    site.write(
        "content/docs/guide.typ",
        "#frontmatter((title: \"Guide\",))\n= Guide\n\n== Setup\none\n\n== Setup\ntwo\n",
    );
    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let html = fs::read_to_string(site.root.join("public/docs/guide/index.html")).unwrap();
    assert!(html.contains(r#"id="guide""#), "{html}");
    // Two headings sluggging alike are disambiguated, not duplicated.
    assert!(html.contains(r#"id="setup""#), "{html}");
    assert!(html.contains(r#"id="setup-2""#), "{html}");
}

#[test]
fn anchors_disabled_leaves_headings_bare() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths { content \"content\"; dist \"public\" }\noutput { html { anchors #false } }\n",
    );
    site.write(
        "content/docs/guide.typ",
        "#frontmatter((title: \"Guide\",))\n== Setup\nbody\n",
    );
    let out = site.run(&["build"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let html = fs::read_to_string(site.root.join("public/docs/guide/index.html")).unwrap();
    assert!(!html.contains(r#"id="setup""#), "{html}");
}
