//! Multi-language sites: a `languages` block turns on i18n, and a `.{code}.typ`
//! filename or a frontmatter `lang` marks a translation.

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
    site.stats();
    assert!(site.exists("public/index.html"));
    assert!(site.exists("public/posts/hello/index.html"));
}

#[test]
fn other_languages_are_prefixed_by_code() {
    let site = multilingual();
    site.stats();
    assert!(site.exists("public/fr/index.html"));
    assert!(site.exists("public/fr/posts/hello/index.html"));
}

#[test]
fn untranslated_pages_are_omitted() {
    let site = multilingual();
    site.stats();
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
    site.write(
        "content/about-de.typ",
        "#let frontmatter = (title: \"Über\", slug: \"about\", lang: \"de\",)\nÜber.\n",
    );
    site.stats();
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
    let err = site.build_error().to_string();
    assert!(err.contains("unknown language"), "{err}");
}

/// A site with a paginated, tagged blog in two languages.
fn tagged_blog() -> Site {
    let site = Site::with(
        r#"
        site "T"
        lang "en"
        languages { fr { name "Français" } }
        paths {
          content "content"
          dist "public"
        }
        content {
          collections {
              blog "blog/**/*.typ" { sort "date"; reverse #true; paginate { template "layout.typ"; size 1 } }
          }
          taxonomies { tags listing=#true template="layout.typ" }
        }
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
    site.stats();
    assert!(site.exists("public/tags/rust/index.html"));
    assert!(site.exists("public/fr/tags/rust/index.html"));
    assert!(site.exists("public/fr/tags/index.html"));
}

#[test]
fn pagination_is_per_language() {
    let site = tagged_blog();
    site.stats();
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
        paths {
          content "content"
          dist "public"
        }
        generate {
          sitemap #true
          feed { formats "rss" }
        }
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
    site.stats();
    assert!(site.exists("public/rss.xml"));
    assert!(site.exists("public/fr/rss.xml"));
}

#[test]
fn html_lang_reflects_the_page_language() {
    let site = hosted();
    site.stats();
    assert!(site.output("index.html").contains("lang=\"en\""));
    assert!(site.output("fr/index.html").contains("lang=\"fr\""));
}

#[test]
fn hreflang_alternates_link_the_translations() {
    let site = hosted();
    site.stats();
    let en = site.output("index.html");
    assert!(en.contains("hreflang=\"en\"") && en.contains("hreflang=\"fr\""));
    assert!(en.contains("hreflang=\"x-default\""));
    assert!(en.contains("property=\"og:locale\" content=\"en\""));
    let map = site.output("sitemap.xml");
    assert!(map.contains("hreflang=\"fr\"") && map.contains("xmlns:xhtml"));
}

/// Editions otherwise pair on `collection/slug`, so a `translation` name is
/// what keeps a renamed edition from becoming a standalone page.
#[test]
fn a_named_translation_pairs_editions_with_different_slugs() {
    let site = Site::with(
        r#"
        site "T"
        url "https://example.com"
        lang "en"
        languages { fr { name "Français" } }
        paths { content "content"; dist "public" }
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\", translation: \"greeting\")\nx\n",
    );
    site.write(
        "content/posts/bonjour.fr.typ",
        "#let frontmatter = (title: \"Bonjour\", translation: \"greeting\")\nx\n",
    );
    site.stats();
    assert!(site.exists("public/posts/hello/index.html"));
    assert!(site.exists("public/fr/posts/bonjour/index.html"));
    let en = site.output("posts/hello/index.html");
    assert!(
        en.contains("hreflang=\"fr\"") && en.contains("/fr/posts/bonjour/"),
        "{en}"
    );
    let fr = site.output("fr/posts/bonjour/index.html");
    assert!(
        fr.contains("hreflang=\"en\"") && fr.contains("/posts/hello/"),
        "{fr}"
    );
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
        site.write(
            f,
            "#let frontmatter = (title: \"H\", template: \"layout.typ\")\nx\n",
        );
    }
    site.stats();
    assert!(site.output("index.html").contains("L=en"));
    let fr = site.output("fr/index.html");
    assert!(fr.contains("L=fr") && fr.contains("S=Lire") && fr.contains("T=2"));
}

/// The `baudelaire:*` modules are served by the bundler, which `js` owns.
#[test]
#[cfg(feature = "js")]
fn i18n_module_inlines_languages_and_strings() {
    let site = Site::with(
        r#"
        site "T"
        lang "en"
        languages { fr { name "Français"; strings { more "Lire la suite" } } }
        paths {
          content "content"
          dist "public"
          assets "assets"
        }
        assets {
          bundle #true
        }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nx\n",
    );
    site.write(
        "assets/main.js",
        "import { languages, strings } from \"baudelaire:i18n\";\nglobalThis.x = { languages, strings };\n",
    );
    site.stats();
    let bundle = site
        .files("public/assets")
        .into_iter()
        .find(|f| f.starts_with("main") && common::has_ext(f, "js"))
        .expect("bundled main.js");
    let js = site.read(&format!("public/assets/{bundle}"));
    assert!(js.contains("Lire la suite") && js.contains("Français"));
}

#[test]
fn an_undeclared_suffix_is_part_of_the_slug() {
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
    site.stats();
    assert!(site.exists("public/notes-fr/index.html"));
    assert!(!site.exists("public/fr/notes/index.html"));
}

#[test]
fn feeds_and_search_are_per_language() {
    let site = Site::with(
        r#"
        site "T"
        url "https://host.test"
        lang "en"
        languages {
            fr { name "Français"; site "T (fr)" }
        }
        paths {
          content "content"
          dist "public"
        }
        generate {
          feed { formats "atom" "json" }
          search { formats "json"; ui #true }
        }
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\", slug: \"hello\", date: datetime(year: 2024, month: 1, day: 2),)\nHi.\n",
    );
    site.write(
        "content/posts/hello.fr.typ",
        "#let frontmatter = (title: \"Bonjour\", slug: \"hello\", date: datetime(year: 2024, month: 1, day: 2),)\nSalut.\n",
    );
    site.stats();

    let en = site.output("atom.xml");
    let fr = site.output("fr/atom.xml");
    assert!(en.contains("<id>https://host.test/atom.xml</id>"), "{en}");
    assert!(
        fr.contains("<id>https://host.test/fr/atom.xml</id>"),
        "{fr}"
    );
    assert!(
        site.output("fr/feed.json")
            .contains("https://host.test/fr/feed.json"),
        "{}",
        site.output("fr/feed.json")
    );
    assert!(fr.contains("T (fr)"), "{fr}");

    let en = site.output("search.json");
    let fr = site.output("fr/search.json");
    assert!(en.contains("Hello") && !en.contains("Bonjour"), "{en}");
    assert!(fr.contains("Bonjour") && !fr.contains("Hello"), "{fr}");
    let client = site.output("fr/search.js");
    assert!(
        client.contains("const INDEX = \"/fr/search.json\""),
        "{client}"
    );
}

#[test]
fn generated_listings_are_translated_and_localized() {
    let site = Site::with(
        r#"
        site "T"
        url "https://host.test"
        lang "en"
        languages {
            fr {
                name "Français"
                strings { previous "← Précédent"; next "Suivant →"; page "page" }
            }
        }
        paths {
          content "content"
          dist "public"
        }
        content {
          taxonomies { tags listing=#true }
        }
        generate {
          sitemap #true
        }
        "#,
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"rust\",),)\nA.\n",
    );
    site.write(
        "content/posts/a.fr.typ",
        "#let frontmatter = (title: \"A fr\", tags: (\"rust\",),)\nA fr.\n",
    );
    site.stats();

    let map = site.output("sitemap.xml");
    assert!(map.contains("/fr/tags/"), "{map}");
    let tags = map
        .split("<url>")
        .find(|entry| entry.contains("<loc>https://host.test/tags/</loc>"))
        .expect("tags index in sitemap");
    assert!(tags.contains("hreflang=\"fr\""), "{tags}");
}

/// A translated page writes the same `#link("b.typ")` as its original and means
/// its own edition.
#[test]
fn typ_links_resolve_to_the_linking_page_s_language() {
    let site = multilingual();
    site.write(
        "content/posts/linker.typ",
        "#let frontmatter = (title: \"Linker\",)\n#link(\"hello.typ\")[go]\n",
    );
    site.write(
        "content/posts/linker.fr.typ",
        "#let frontmatter = (title: \"Lieur\",)\n#link(\"hello.typ\")[aller]\n",
    );
    site.stats();

    assert!(
        site.output("posts/linker/index.html")
            .contains("\"/posts/hello/\""),
        "{}",
        site.output("posts/linker/index.html")
    );
    let fr = site.output("fr/posts/linker/index.html");
    assert!(fr.contains("\"/fr/posts/hello/\""), "{fr}");
}

#[test]
fn unicode_names_survive_slugging() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\ncontent {\n  taxonomies { tags listing=#true }\n}\n",
    );
    site.write(
        "content/posts/café.typ",
        "#let frontmatter = (title: \"Café\", tags: (\"日本語\",),)\n= Ünïcödé heading\n",
    );
    site.stats();

    assert!(
        site.exists("public/posts/café/index.html"),
        "{:?}",
        site.files("public/posts")
    );
    let html = site.output("posts/café/index.html");
    assert!(html.contains("id=\"ünïcödé-heading\""), "{html}");
    assert!(
        site.exists("public/tags/日本語/index.html"),
        "{:?}",
        site.files("public/tags")
    );
}
