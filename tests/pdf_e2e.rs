//! A PDF beside every page.

#![cfg(feature = "pdf")]

mod common;

use common::Site;

/// A PDF's own header, so a test asserts on a real document rather than on the
/// file merely existing.
fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-")
}

/// A site with a paged template that prints the page's title and its body, so
/// the PDF is unmistakably this page and not a blank default.
fn site(config: &str) -> Site {
    let site = Site::with(&format!(
        r#"
        site "T"
        url "https://example.com"
        paths {{ content "content"; dist "public"; templates "templates" }}
        generate {{ {config} }}
        "#
    ));
    site.write(
        "templates/print.typ",
        "#let print(page, body) = {\n\
         set document(title: page.frontmatter.title)\n\
         heading(level: 1, page.frontmatter.title)\n\
         body\n\
         }\n",
    );
    site
}

#[test]
fn a_pdf_is_written_beside_the_page_and_linked_from_it() {
    let site = site(r#"pdf { pages { template "print.typ" } }"#);
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();

    let pdf = std::fs::read(site.path("public/posts/hello.pdf")).expect("pdf");
    assert!(is_pdf(&pdf), "not a PDF: {:?}", &pdf[..8.min(pdf.len())]);
    assert!(
        site.exists("public/posts/hello/index.html"),
        "the HTML is still written"
    );

    let html = site.output("posts/hello/index.html");
    assert!(
        html.contains(r#"href="https://example.com/posts/hello.pdf""#),
        "{html}"
    );
    assert!(html.contains(r#"type="application/pdf""#), "{html}");
}

/// The bytes must not move on their own. Typst stamps an export timestamp and a
/// document identifier, both of which default to the instant of the export: two
/// builds of an unchanged page produced two different files, so every deploy
/// re-uploaded every PDF on the site.
#[test]
fn an_unchanged_page_exports_the_same_bytes() {
    let site = site(r#"pdf { pages { template "print.typ" } }"#);
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();
    let before = std::fs::read(site.path("public/posts/hello.pdf")).expect("pdf");

    // Force a recompile rather than a cache hit: a hit would keep the file
    // untouched and prove nothing about the exporter.
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody ",
    );
    site.stats();
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    let stats = site.stats();

    assert_eq!((stats.pages, stats.cached), (1, 0), "the page recompiled");
    let after = std::fs::read(site.path("public/posts/hello.pdf")).expect("pdf");
    assert_eq!(before, after, "the same page exported different bytes");
}

/// The paged template is handed the same `page` dict the layout gets, so
/// anything a template can say on screen it can say on paper.
#[test]
fn the_paged_template_gets_the_page_bindings() {
    let site = site(r#"pdf { pages { template "print.typ" } }"#);
    site.write(
        "templates/print.typ",
        "#let print(page, body) = [#page.lang #page.reading.words #page.date.iso]\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\", date: datetime(year: 2026, month: 7, day: 30),)\n\
         one two three",
    );
    let stats = site.stats();

    assert_eq!(stats.pages, 1, "the bindings resolved, so the page built");
    assert!(is_pdf(
        &std::fs::read(site.path("public/posts/hello.pdf")).expect("pdf")
    ));
}

/// Generated listings get no PDF: a tag index is a table of contents for a
/// site, not a document anyone prints.
#[test]
fn generated_listings_get_no_pdf() {
    let site = site(r#"pdf { pages { template "print.typ" } }"#);
    site.write("config.kdl", &{
        let mut config = site.read("config.kdl");
        config.push_str("content {\n  taxonomies {\n    tags listing=#true\n  }\n}\n");
        config
    });
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"rust\",),)\nbody",
    );
    site.stats();

    assert!(site.exists("public/posts/a.pdf"));
    assert!(
        site.exists("public/tags/rust/index.html"),
        "term page built"
    );
    assert!(!site.exists("public/tags/rust.pdf"));
}

/// The sidecar rules apply: a deleted PDF makes its page stale, and a page
/// nothing touched keeps the file the last build wrote.
#[test]
fn a_deleted_pdf_is_re_exported_and_an_untouched_one_survives() {
    let site = site(r#"pdf { pages { template "print.typ" } }"#);
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();

    let cached = site.stats();
    assert_eq!((cached.pages, cached.cached), (1, 1), "nothing changed");
    assert!(site.exists("public/posts/hello.pdf"), "the PDF survives");

    std::fs::remove_file(site.path("public/posts/hello.pdf")).expect("pdf removed");
    let redrawn = site.stats();

    assert_eq!(
        (redrawn.pages, redrawn.cached),
        (1, 0),
        "the PDF is gone, so the page cannot be reused"
    );
    assert!(site.exists("public/posts/hello.pdf"), "re-exported");
}

/// Editing a module the paged template imports has to re-export the PDF: the
/// page's own compile never read it, so nothing else would notice.
#[test]
fn editing_a_module_the_paged_template_imports_re_exports_the_pdf() {
    let site = site(r#"pdf { pages { template "print.typ" } }"#);
    site.write("templates/print-style.typ", "#let size = 11pt\n");
    site.write(
        "templates/print.typ",
        "#import \"print-style.typ\": size\n\
         #let print(page, body) = { set text(size: size); body }\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();
    let before = std::fs::read(site.path("public/posts/hello.pdf")).expect("pdf");

    site.write("templates/print-style.typ", "#let size = 22pt\n");
    let stats = site.stats();

    assert_eq!(
        (stats.pages, stats.cached),
        (1, 0),
        "the page exports from the edited module, so it cannot be reused"
    );
    let after = std::fs::read(site.path("public/posts/hello.pdf")).expect("pdf");
    assert_ne!(before, after, "the PDF should have been re-exported");
}

/// A site with a bundle template that prints the document title and every
/// entry, so the PDF is unmistakably the whole run of pages.
fn bound(config: &str) -> Site {
    let site = site(config);
    site.write(
        "templates/book.typ",
        "#let book(doc, entries) = {\n\
         heading(level: 1, doc.title)\n\
         for e in entries {\n\
         heading(level: 2, e.page.frontmatter.title)\n\
         e.body\n\
         }\n\
         }\n",
    );
    site
}

#[test]
fn a_collection_and_the_site_bind_into_documents_of_their_own() {
    let site = bound(r#"pdf { bundle { template "book.typ"; collections "posts"; site #true } }"#);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nfirst",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nsecond",
    );
    site.stats();

    assert!(is_pdf(
        &std::fs::read(site.path("public/posts.pdf")).expect("the collection")
    ));
    assert!(is_pdf(
        &std::fs::read(site.path("public/site.pdf")).expect("the site")
    ));
    // The per-page half is a separate block, so asking for a bundle alone
    // writes no per-page PDF.
    assert!(!site.exists("public/posts/a.pdf"));
}

/// A bundle belongs to no page, so it cannot ride a page's cache entry: editing
/// any bound page has to re-export it, and a build that changed nothing must
/// not.
#[test]
fn a_bundle_re_exports_when_any_of_its_pages_changes() {
    let site = bound(r#"pdf { bundle { template "book.typ"; collections "posts" } }"#);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nfirst",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nsecond",
    );
    site.stats();
    let before = std::fs::read(site.path("public/posts.pdf")).expect("bundle");

    site.stats();
    let idle = std::fs::read(site.path("public/posts.pdf")).expect("bundle");
    assert_eq!(before, idle, "nothing changed, so nothing was re-exported");

    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nsecond, rewritten",
    );
    site.stats();

    let after = std::fs::read(site.path("public/posts.pdf")).expect("bundle");
    assert_ne!(before, after, "an edited page has to reach the document");
}

/// Adding a page changes no file the last build read, so only the module text
/// (which names every page it binds, in order) can catch it.
#[test]
fn a_new_page_joins_the_bundle() {
    let site = bound(r#"pdf { bundle { template "book.typ"; collections "posts" } }"#);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nfirst",
    );
    site.stats();
    let before = std::fs::read(site.path("public/posts.pdf")).expect("bundle");

    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nsecond",
    );
    site.stats();

    let after = std::fs::read(site.path("public/posts.pdf")).expect("bundle");
    assert_ne!(before, after, "the new page has to be in the document");
}

/// A markdown page joins a bundle as the Typst it lowered to, exactly as it
/// reaches its own compile and its own sidecar.
///
/// Bound by `#include` instead, typst was handed raw markdown: a page carrying a
/// heading line failed the build outright (`expected expression`, pointed into
/// the `.md`), and one without built green and bound the file verbatim, fences
/// and all, with no frontmatter -- so the document had no title for it. The
/// template here refuses the second outcome, and the build refuses the first.
#[test]
#[cfg(feature = "markdown")]
fn a_markdown_page_binds_as_the_typst_it_lowered_to() {
    let site = bound(r#"pdf { bundle { template "book.typ"; collections "posts" } }"#);
    site.write(
        "templates/book.typ",
        "#let book(doc, entries) = {\n\
         heading(level: 1, doc.title)\n\
         for e in entries {\n\
         assert(\"title\" in e.page.frontmatter, message: repr(e.page.frontmatter))\n\
         heading(level: 2, e.page.frontmatter.title)\n\
         e.body\n\
         }\n\
         }\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nfirst",
    );
    site.write(
        "content/posts/b.md",
        ";;;\ntitle \"B\"\n;;;\n\n## A heading\n\nSecond, with **emphasis**.\n",
    );
    let stats = site.stats();

    assert_eq!(stats.pages, 2, "both pages built");
    assert!(is_pdf(
        &std::fs::read(site.path("public/posts.pdf")).expect("bundle")
    ));
}

/// The sidecar rule applies to bundles too: a file deleted from `dist` while the
/// cache stays warm has to come back.
#[test]
fn a_deleted_bundle_is_re_exported() {
    let site = bound(r#"pdf { bundle { template "book.typ"; collections "posts" } }"#);
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\nfirst",
    );
    site.stats();

    std::fs::remove_file(site.path("public/posts.pdf")).expect("removed");
    site.stats();

    assert!(site.exists("public/posts.pdf"), "re-exported");
}

/// Every starter shape ships a `print.typ`, and `--with pdf` is what turns it
/// on. The template is inert until then, which is exactly how one ships broken:
/// scaffold a site, ask for PDFs, and build it.
#[test]
fn the_scaffolded_print_template_builds() {
    for shape in ["blog", "docs", "book", "minimal"] {
        let t = Site::new();
        let out = t.run(&[
            "init",
            "-r",
            t.root.to_str().expect("root"),
            "--template",
            shape,
            "--with",
            "pdf",
        ]);
        assert!(
            out.status.success(),
            "`init --template {shape}` failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(t.exists("templates/print.typ"), "`{shape}` ships one");

        let out = t.run(&["build"]);
        assert!(
            out.status.success(),
            "`{shape}` did not build: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(t.exists("public/index.pdf"), "`{shape}` wrote no home PDF");
        assert!(is_pdf(
            &std::fs::read(t.path("public/index.pdf")).expect("pdf")
        ));

        // The shapes a bundle is *for* ship a template for it, and it is inert
        // until a target is named: the same way one ships broken.
        if !t.exists("templates/book.typ") {
            continue;
        }
        let mut config = t.read("config.kdl");
        config.push_str("generate {\n  pdf { bundle { template \"book.typ\"; site #true } }\n}\n");
        t.write("config.kdl", &config);
        let out = t.run(&["build"]);
        assert!(
            out.status.success(),
            "`{shape}` did not bundle: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(is_pdf(
            &std::fs::read(t.path("public/site.pdf")).expect("the bundle")
        ));
    }
}

/// Off unless asked for: this is a second compile of every page, and this one
/// lays the whole document out.
#[test]
fn no_pdf_without_the_block() {
    let site = site("");
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();

    assert!(!site.exists("public/posts/hello.pdf"));
    let html = site.output("posts/hello/index.html");
    assert!(!html.contains("application/pdf"), "{html}");
}
