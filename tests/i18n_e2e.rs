//! Multi-language sites: a `languages` block turns on i18n. A `.{code}.typ`
//! filename (or a frontmatter `lang`) marks a translation; the default language
//! keeps clean root URLs while others sit under `/{code}/`. Untranslated pages
//! are simply omitted for a language.

mod common;

use common::Site;

/// An English (default) site declaring French and German, with a French home
/// and a French translation of one post.
fn multilingual() -> Site {
    let site = Site::with(
        r#"
        site "T"
        lang "en"
        languages {
            fr { name "Français" }
            de { name "Deutsch" }
        }
        paths { content "content"; dist "public" }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nWelcome.\n",
    );
    site.write(
        "content/index.fr.typ",
        "#let frontmatter = (title: \"Accueil\",)\nBienvenue.\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\", slug: \"hello\",)\nHi.\n",
    );
    site.write(
        "content/posts/hello.fr.typ",
        "#let frontmatter = (title: \"Bonjour\", slug: \"hello\",)\nSalut.\n",
    );
    site
}

#[test]
fn default_language_stays_at_the_root() {
    let site = multilingual();
    site.build();
    assert!(site.exists("public/index.html"));
    assert!(site.exists("public/posts/hello/index.html"));
}

#[test]
fn other_languages_are_prefixed_by_code() {
    let site = multilingual();
    site.build();
    assert!(site.exists("public/fr/index.html"));
    assert!(site.exists("public/fr/posts/hello/index.html"));
}

#[test]
fn untranslated_pages_are_omitted() {
    // German is declared but has no content, so nothing builds under /de/.
    let site = multilingual();
    site.build();
    assert!(!site.exists("public/de/index.html"));
    assert!(!site.exists("public/de/posts/hello/index.html"));
}

#[test]
fn frontmatter_lang_overrides_the_filename() {
    let site = multilingual();
    site.write(
        "content/about.typ",
        "#let frontmatter = (title: \"About\",)\nAbout.\n",
    );
    // No `.de` suffix; the language comes from frontmatter instead.
    site.write(
        "content/about-de.typ",
        "#let frontmatter = (title: \"Über\", slug: \"about\", lang: \"de\",)\nÜber.\n",
    );
    site.build();
    assert!(site.exists("public/about/index.html"));
    assert!(site.exists("public/de/about/index.html"));
}

#[test]
fn unknown_language_is_rejected() {
    let site = multilingual();
    site.write(
        "content/bad.typ",
        "#let frontmatter = (title: \"Bad\", lang: \"xx\",)\n",
    );
    let out = site.run(&["build"]);
    assert!(!out.status.success(), "build should reject an unknown language");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown language"), "{stderr}");
}

/// A site with a paginated, tagged blog in two languages.
fn tagged_blog() -> Site {
    let site = Site::with(
        r#"
        site "T"
        lang "en"
        languages { fr { name "Français" } }
        paths { content "content"; dist "public" }
        collections {
            blog "blog/**/*.typ" sort="date" reverse=#true list="layout.typ" paginate=1
        }
        taxonomies { tags index=#true template="layout.typ" }
        "#,
    );
    site.write("templates/layout.typ", "#let layout(page, body) = body\n");
    for (file, title, date) in [
        ("content/blog/a.typ", "A", 2),
        ("content/blog/a.fr.typ", "A-fr", 2),
        ("content/blog/b.typ", "B", 3),
    ] {
        site.write(
            file,
            &format!(
                "#let frontmatter = (title: \"{title}\", slug: \"{}\", \
                 date: datetime(year: 2024, month: 1, day: {date}), tags: (\"rust\",),)\nx\n",
                title.trim_end_matches("-fr").to_lowercase()
            ),
        );
    }
    site
}

#[test]
fn taxonomies_do_not_merge_across_languages() {
    let site = tagged_blog();
    site.build();
    // Each language gets its own tag pages; the term is never merged.
    assert!(site.exists("public/tags/rust/index.html"));
    assert!(site.exists("public/fr/tags/rust/index.html"));
    assert!(site.exists("public/fr/tags/index.html"));
}

#[test]
fn pagination_is_per_language() {
    let site = tagged_blog();
    site.build();
    // English has two posts at paginate=1 -> page 1 + page 2; French has one.
    assert!(site.exists("public/blog/index.html"));
    assert!(site.exists("public/blog/page/2/index.html"));
    assert!(site.exists("public/fr/blog/index.html"));
    assert!(!site.exists("public/fr/blog/page/2/index.html"));
}

/// A translated site with a base URL, so feeds, sitemap, and hreflang are on.
fn hosted() -> Site {
    let site = Site::with(
        r#"
        site "T"
        url "https://ex.test"
        lang "en"
        languages { fr { name "Français" } }
        paths { content "content"; dist "public" }
        output { sitemap #true; feed { formats "rss" } }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nHi.\n",
    );
    site.write(
        "content/index.fr.typ",
        "#let frontmatter = (title: \"Accueil\",)\nSalut.\n",
    );
    site.write(
        "content/posts/p.typ",
        "#let frontmatter = (title: \"P\", slug: \"p\", date: datetime(year: 2024, month: 1, day: 1),)\nx\n",
    );
    site.write(
        "content/posts/p.fr.typ",
        "#let frontmatter = (title: \"P-fr\", slug: \"p\", date: datetime(year: 2024, month: 1, day: 1),)\nx\n",
    );
    site
}

#[test]
fn feeds_are_emitted_per_language() {
    let site = hosted();
    site.build();
    assert!(site.exists("public/rss.xml"));
    assert!(site.exists("public/fr/rss.xml"));
}

#[test]
fn html_lang_reflects_the_page_language() {
    let site = hosted();
    site.build();
    assert!(site.output("index.html").contains("lang=\"en\""));
    assert!(site.output("fr/index.html").contains("lang=\"fr\""));
}

#[test]
fn hreflang_alternates_link_the_translations() {
    let site = hosted();
    site.build();
    let en = site.output("index.html");
    // Both editions plus an x-default, on the page and in the sitemap.
    assert!(en.contains("hreflang=\"en\"") && en.contains("hreflang=\"fr\""));
    assert!(en.contains("hreflang=\"x-default\""));
    assert!(en.contains("property=\"og:locale\" content=\"en\""));
    let map = site.output("sitemap.xml");
    assert!(map.contains("hreflang=\"fr\"") && map.contains("xmlns:xhtml"));
}

#[test]
fn template_receives_lang_translations_and_strings() {
    let site = Site::with(
        r#"
        site "T"
        lang "en"
        languages { fr { name "Français"; strings { more "Lire" } } }
        paths { content "content"; dist "public" }
        "#,
    );
    site.write(
        "templates/layout.typ",
        "#let layout(page, body) = [L=#page.lang S=#page.strings.at(\"more\", default: \"-\") \
         T=#page.translations.len() #body]\n",
    );
    for f in ["content/index.typ", "content/index.fr.typ"] {
        site.write(f, "#let frontmatter = (title: \"H\", template: \"layout.typ\")\nx\n");
    }
    site.build();
    assert!(site.output("index.html").contains("L=en"));
    let fr = site.output("fr/index.html");
    assert!(fr.contains("L=fr") && fr.contains("S=Lire") && fr.contains("T=2"));
}

#[test]
fn i18n_module_inlines_languages_and_strings() {
    let site = Site::with(
        r#"
        site "T"
        lang "en"
        languages { fr { name "Français"; strings { more "Lire la suite" } } }
        paths { content "content"; dist "public"; assets "assets" }
        output { assets { bundle #true } }
        "#,
    );
    site.write("content/index.typ", "#let frontmatter = (title: \"H\",)\nx\n");
    site.write(
        "assets/main.js",
        "import { languages, strings } from \"baudelaire:i18n\";\nglobalThis.x = { languages, strings };\n",
    );
    site.build();
    let bundle = site
        .files("public/assets")
        .into_iter()
        .find(|f| f.starts_with("main") && f.ends_with(".js"))
        .expect("bundled main.js");
    let js = site.read(&format!("public/assets/{bundle}"));
    assert!(js.contains("Lire la suite") && js.contains("Français"));
}

#[test]
fn an_undeclared_suffix_is_part_of_the_slug() {
    // A single-language site: a dotted stem is not a language, so it survives
    // into the slug rather than being peeled off.
    let site = Site::with(
        r#"
        site "T"
        paths { content "content"; dist "public" }
        "#,
    );
    site.write(
        "content/notes.fr.typ",
        "#let frontmatter = (title: \"Notes\",)\nPlain.\n",
    );
    site.build();
    assert!(site.exists("public/notes-fr/index.html"));
    assert!(!site.exists("public/fr/notes/index.html"));
}
