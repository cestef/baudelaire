//! What a URL becomes: style, base path, permalinks, and the files behind them.

use super::{code, parse};
use crate::config::Config;
/// `url` is what every absolute URL the site emits is built by concatenation
/// onto, so a value that is not one produced `<loc>example.com/a/</loc>` in the
/// sitemap, the same in every feed `<id>` and canonical tag, out of a green
/// build.
#[test]
fn err_a_site_url_without_a_scheme_is_refused() {
    for bad in [
        "example.com",
        "//example.com",
        "https://",
        "https://a b.com",
    ] {
        let err = Config::parse(&format!("url {bad:?}")).unwrap_err();
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("is not an absolute URL"),
            "{bad}: {rendered}"
        );
    }
}

/// ...and any scheme is the author's business: `url` is served to readers, not
/// a host credentials are sent to. That is `typst { registry }`'s rule, not
/// this one's.
#[test]
fn a_site_url_may_name_any_scheme() {
    for good in [
        "https://example.com",
        "http://example.com",
        "http://localhost:8080/docs",
        "https://example.com/docs/",
    ] {
        assert_eq!(
            parse(&format!("url {good:?}")).url.as_deref(),
            Some(good),
            "{good} should parse"
        );
    }
}

#[test]
fn url_style_parsed_from_links_block() {
    use crate::config::UrlStyle;
    assert_eq!(
        parse("links {\n  style \"clean\"\n}").links.style,
        UrlStyle::Clean
    );
    assert_eq!(
        parse("links {\n  style \"flat\"\n}").links.style,
        UrlStyle::Flat
    );
}

#[test]
fn err_unknown_url_style() {
    let err = Config::parse("links {\n  style \"pretty\"\n}").unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    // The value table drives the "did you mean" hint, listing the valid styles.
    assert!(rendered.contains("unknown value `pretty`"), "{rendered}");
    assert!(rendered.contains("`clean`, `flat`"), "{rendered}");
}

#[test]
fn err_unknown_permalink_placeholder_is_spanned() {
    let text = "content {\n  collections {\n    posts { permalink \"/{bogus}/\" }\n  }\n}\n";
    let err = Config::parse(text).unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(
        rendered.contains("unknown permalink placeholder `bogus`"),
        "{rendered}"
    );
    assert!(rendered.contains("valid placeholders"), "{rendered}");
    // the label excerpts config.kdl: the error is spanned at parse time
    assert!(rendered.contains("permalink "), "{rendered}");
}

#[test]
fn err_unterminated_permalink_placeholder() {
    let err =
        Config::parse("content {\n  collections {\n    posts { permalink \"/{slug\" }\n  }\n}\n")
            .unwrap_err();
    assert!(err.to_string().contains("unterminated `{slug`"), "{err}");
}

#[test]
fn err_permalink_parent_dir_segment() {
    let err = Config::parse(
        "content {\n  collections {\n    posts { permalink \"/../{slug}/\" }\n  }\n}\n",
    )
    .unwrap_err();
    assert!(err.to_string().contains("must not contain `..`"), "{err}");
}

/// `mount` and `prefix` are pieces of the index's permalink, so the rule
/// `permalink` is held to is theirs too. `Config::segments` drops a `..` before
/// anything is written, so what these produced was never an escaped file: it was
/// an index published at a URL nobody asked for, out of a green build.
#[test]
fn err_a_paginate_path_climbing_out_is_refused() {
    for config in [
        "content {\n  collections {\n    posts { paginate { mount \"/../elsewhere/\" } }\n  }\n}\n",
        "content {\n  collections {\n    posts { paginate { prefix \"../..\" } }\n  }\n}\n",
    ] {
        let err = Config::parse(config).unwrap_err();
        assert!(err.to_string().contains("must not contain `..`"), "{err}");
    }
    // The shapes they are actually written in still parse.
    let cfg = parse(
        "content {\n  collections {\n    posts { paginate { mount \"/\"; prefix \"\" } }\n  }\n}\n",
    );
    let posts = &cfg.content.collections[0].1;
    assert_eq!(posts.paginate.mount.as_deref(), Some("/"));
    assert_eq!(posts.paginate.prefix, "");
}

/// A feed's file name is joined onto `dist` by the emitter, so it is held to the
/// containment rule every other generated file name is: `rss "../../pwned.xml"`
/// wrote two directories above the project root and reported success.
#[test]
fn err_a_feed_name_leaving_dist_is_refused() {
    for name in ["../../pwned.xml", "/etc/passwd", "a/../../b.xml", ""] {
        let config = format!("generate {{\n  feed {{\n    names {{ rss {name:?} }}\n  }}\n}}\n");
        let err = Config::parse(&config).unwrap_err();
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("would be written outside the output directory"),
            "{name}: {rendered}"
        );
    }
    // A name, or a name in a subdirectory, is what the key is for.
    let cfg = parse(
        "generate {\n  feed {\n    names { rss \"index.xml\"; atom \"feeds/atom.xml\" }\n  }\n}\n",
    );
    assert_eq!(
        cfg.generate.feed.file(crate::config::FeedKind::Rss),
        "index.xml"
    );
    assert_eq!(
        cfg.generate.feed.file(crate::config::FeedKind::Atom),
        "feeds/atom.xml"
    );
}

#[test]
fn destination_never_escapes_dist() {
    let cfg = parse("");
    let dist = cfg.paths.dist.clone();
    for url in ["/../../etc/passwd/", "/posts/../../secret/"] {
        let written = cfg.destination(url);
        assert!(written.starts_with(&dist), "{url} -> {}", written.display());
        assert!(
            !written.components().any(|c| c.as_os_str() == ".."),
            "{url} -> {}",
            written.display()
        );
    }
}

/// A translated not-found page must stay a flat `404.html` inside its language
/// scope: `dist/fr/404/index.html` is not served as not-found by any host.
#[test]
fn a_localized_404_stays_a_flat_page() {
    let cfg = parse("lang \"en\"\nlanguages {\n  fr { }\n}");
    assert_eq!(cfg.destination("/404/"), cfg.paths.dist.join("404.html"));
    assert_eq!(
        cfg.destination("/fr/404/"),
        cfg.paths.dist.join("fr/404.html")
    );
    // Only a language scope counts: an ordinary page that happens to be called
    // `404` keeps the site's URL style.
    assert_eq!(
        cfg.destination("/notes/404/"),
        cfg.paths.dist.join("notes/404/index.html")
    );
    assert_eq!(
        cfg.destination("/posts/4040/"),
        cfg.paths.dist.join("posts/4040/index.html")
    );
}

#[test]
fn base_path_is_the_url_path_component() {
    assert_eq!(parse("url \"https://host.test/docs\"").base_path(), "/docs");
    assert_eq!(parse("url \"https://host.test/a/b\"").base_path(), "/a/b");
    assert_eq!(
        parse("url \"https://host.test/docs/\"").base_path(),
        "/docs"
    );
    assert_eq!(parse("url \"https://host.test\"").base_path(), "");
    assert_eq!(parse("url \"https://host.test/\"").base_path(), "");
    assert_eq!(parse("").base_path(), "");
}

#[test]
fn prefixed_shifts_only_root_absolute_paths() {
    let cfg = parse("url \"https://host.test/docs\"");
    assert_eq!(cfg.prefixed("/guide/"), "/docs/guide/");
    assert_eq!(cfg.prefixed("//cdn/x"), "//cdn/x");
    assert_eq!(cfg.prefixed("https://x/y"), "https://x/y");
    assert_eq!(cfg.prefixed("#frag"), "#frag");
    assert_eq!(
        parse("url \"https://host.test\"").prefixed("/guide/"),
        "/guide/"
    );
}

/// Absolute URLs are percent-encoded: `<loc>` and a feed's `<id>` are specified
/// as URIs, and slugs now carry raw UTF-8.
#[test]
fn absolute_urls_are_percent_encoded() {
    let cfg = parse("url \"https://host.test\"");
    let base = cfg.base().expect("base");
    assert_eq!(
        base.join("/posts/café/"),
        "https://host.test/posts/caf%C3%A9/"
    );
    // Path structure and the sub-delimiters a URL legitimately uses survive.
    assert_eq!(base.join("/a/b-c_d~e/"), "https://host.test/a/b-c_d~e/");
    // An already-encoded path is not encoded twice.
    assert_eq!(
        base.join("/posts/caf%C3%A9/"),
        "https://host.test/posts/caf%C3%A9/"
    );
}

/// ...and decoding is its inverse, which is what the dev server applies to an
/// incoming request path.
#[test]
fn percent_round_trips() {
    use crate::config::Percent;
    for path in ["/posts/café/", "/a b/", "/日本語/", "/plain/"] {
        assert_eq!(Percent::decode(&Percent::encode(path)), path);
    }
}

/// A `dist` that contains the sources is what `paths { dist "." }` produced: the
/// prune sweep deleted `config.kdl`, the content tree and every unrelated file in
/// the project, and reported a successful build.
#[test]
fn dist_containing_a_source_directory_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let cfg = parse("paths { dist \".\" }");
    let (key, _) = cfg.paths.swallowed(root).expect("`.` swallows the project");
    assert_eq!(key, "content");

    // Nesting one level down is the same hazard spelled less obviously.
    let cfg = parse("paths { dist \"out\"; content \"out/content\" }");
    let (key, _) = cfg.paths.swallowed(root).expect("nested content swallowed");
    assert_eq!(key, "content");

    // As is a `dist` above the project, which reaches the sources by climbing.
    let cfg = parse("paths { dist \"..\" }");
    assert!(cfg.paths.swallowed(root).is_some());

    // A source directory that *equals* `dist` is swallowed whole.
    let cfg = parse("paths { dist \"public\"; static \"public\" }");
    let (key, _) = cfg.paths.swallowed(root).expect("static swallowed");
    assert_eq!(key, "static");
}

/// The conventional layout, and a `dist` outside the project, both stand: the
/// guard refuses containment, not any particular location.
#[test]
fn a_dist_beside_the_sources_is_accepted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    assert_eq!(Config::default().paths.swallowed(root), None);
    assert_eq!(
        parse("paths { dist \"../site\" }").paths.swallowed(root),
        None
    );
    assert_eq!(
        parse("paths { dist \"public/dist\"; assets \"src/assets\" }")
            .paths
            .swallowed(root),
        None
    );
}

/// A redirect is a line, not a pair: the target is its one positional and the
/// status an attribute, so every pair a site already wrote keeps its meaning.
#[test]
fn a_redirect_carries_its_own_status() {
    let cfg =
        parse("redirect {\n  \"/moved/\" \"/new/\"\n  \"/temp/\" \"/elsewhere/\" status=302\n}");
    let [(first, moved), (second, temp)] = cfg.redirect.as_slice() else {
        panic!("two redirects, got {:?}", cfg.redirect);
    };
    assert_eq!(first, "/moved/");
    assert_eq!(moved.target, "/new/");
    assert_eq!(moved.status, 301, "permanent unless it says otherwise");
    assert!(!moved.needs_rules());

    assert_eq!(second, "/temp/");
    assert_eq!(temp.target, "/elsewhere/");
    assert_eq!(temp.status, 302);
    assert!(temp.needs_rules(), "a stub cannot say 302");
}

#[test]
fn err_a_redirect_status_outside_the_class_is_refused() {
    // Not a redirect at all, and a typo respectively.
    for text in [
        "redirect {\n  \"/a/\" \"/b/\" status=200\n}",
        "redirect {\n  \"/a/\" \"/b/\" status=3012\n}",
    ] {
        assert_eq!(code(text), "baudelaire::config::out_of_range", "{text}");
    }
}

/// One old path forwards to one place. Two lines claiming it is a config typo,
/// named where every other repeated id is.
#[test]
fn err_a_repeated_old_path_is_refused() {
    assert_eq!(
        code("redirect {\n  \"/a/\" \"/b/\"\n  \"/a/\" \"/c/\"\n}"),
        "baudelaire::config::duplicate_id"
    );
}
