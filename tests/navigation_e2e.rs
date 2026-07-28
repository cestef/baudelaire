//! Client-side navigation: the single-file export and the SPA runtime.
//!
//! Both are additive: the ordinary multi-file site is still written, and these
//! only add artifacts beside it.

mod common;

use common::Site;

/// A two-page site that links between its pages and loads a stylesheet, with
/// both navigation outputs on and assets inlined (what a single file needs).
fn site() -> Site {
    let site = Site::with(
        r#"
        site "T"
        paths {
          content "content"
          dist "public"
          assets "assets"
        }
        html {
          embed #true
        }
        navigation {
          standalone { file "site.html" }
          spa { root "main"; prefetch "visible" }
        }
        "#,
    );
    site.write("assets/style.css", "body { color: rebeccapurple }\n");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\n\
         #html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/style.css\"))\n\
         #html.elem(\"main\")[Welcome, see #link(\"about.typ\")[about].]\n",
    );
    site.write(
        "content/about.typ",
        "#let frontmatter = (title: \"About\",)\n\
         #html.elem(\"link\", attrs: (rel: \"stylesheet\", href: \"/assets/style.css\"))\n\
         #html.elem(\"main\")[All about it.]\n",
    );
    site
}

/// The route table the exported file carries, parsed the way its own script
/// parses it: `<\/` back to `</`, then JSON.
fn routes(html: &str) -> serde_json::Value {
    let after = html
        .split_once("baudelaire-routes")
        .expect("routes island")
        .1;
    let json = after.split_once('>').expect("island opens").1;
    let json = json.split_once("</script>").expect("island closes").0;
    serde_json::from_str(&json.replace("<\\/", "</")).expect("valid JSON")
}

#[test]
fn the_export_is_one_file_holding_every_page() {
    let site = site();
    site.stats();
    let html = site.output("site.html");

    let routes = routes(&html);
    assert_eq!(routes["/"]["title"], "Home");
    assert_eq!(routes["/about/"]["title"], "About");
    assert!(
        routes["/about/"]["html"]
            .as_str()
            .expect("route markup")
            .contains("All about it."),
        "{routes:?}"
    );

    // The entry route is rendered into the body as well, so the file shows
    // something with JavaScript off.
    assert!(html.contains("Welcome"), "entry not rendered: {html}");
    // ..and the multi-file site is still there beside it.
    assert!(site.output("about/index.html").contains("All about it."));
}

/// A stylesheet both pages link is the document's, so the file inlines it once
/// rather than once per route. That is the difference between an export that
/// grows with the site and one that grows with the site squared.
#[test]
fn a_shared_stylesheet_is_inlined_once() {
    let site = site();
    site.stats();
    let html = site.output("site.html");

    let inlined = html.matches("data:text/css").count();
    assert_eq!(inlined, 1, "stylesheet inlined {inlined} times");
    // No route repeats it: the shell carries it for all of them.
    let routes = routes(&html);
    for (url, route) in routes.as_object().expect("route table") {
        let markup = route["html"].as_str().expect("route markup");
        assert!(!markup.contains("data:text/css"), "{url} repeats it");
    }
}

#[test]
fn the_spa_client_is_emitted_with_the_configured_options() {
    let site = site();
    site.stats();
    let js = site.output("spa.js");

    assert!(js.contains(r#"const ROOT = "main";"#), "{js}");
    assert!(js.contains(r#"const PREFETCH = "visible";"#), "{js}");
    assert!(js.contains("mountSpa();"), "auto-mounts: {js}");
}

/// The two are independent features, not two halves of one. The export carries
/// its own router, so it navigates with no `spa.js` anywhere in the build.
#[test]
fn the_export_stands_alone_without_the_spa_runtime() {
    let site = Site::with(
        r#"
        site "T"
        paths {
          content "content"
          dist "public"
        }
        html {
          embed #true
        }
        navigation {
          standalone { file "site.html" }
        }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nHome.\n",
    );
    site.write(
        "content/about.typ",
        "#let frontmatter = (title: \"About\",)\nAbout.\n",
    );
    site.stats();

    let html = site.output("site.html");
    assert!(!site.exists("public/spa.js"), "no runtime emitted");
    assert!(
        html.contains("mountRouter("),
        "carries its own router: {html}"
    );
    assert_eq!(routes(&html).as_object().expect("route table").len(), 2);
}

/// ..and the runtime is useful on its own, with no export beside it.
#[test]
fn the_spa_runtime_stands_alone_without_the_export() {
    let site = Site::with(
        r#"
        site "T"
        paths {
          content "content"
          dist "public"
        }
        navigation {
          spa { root "main" }
        }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nHome.\n",
    );
    site.stats();

    assert!(!site.exists("public/site.html"));
    assert!(site.output("spa.js").contains("mountSpa();"));
}

/// The zero-JavaScript path: rules in the head, nothing shipped.
#[test]
fn speculation_rules_land_in_the_head() {
    let site = Site::with(
        r#"
        site "T"
        paths {
          content "content"
          dist "public"
        }
        navigation {
          speculation { prefetch "eager"; prerender "conservative" }
        }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nHome.\n",
    );
    site.stats();

    let html = site.output("index.html");
    let rules = html
        .split_once(r#"type="speculationrules">"#)
        .expect("rules script")
        .1
        .split_once("</script>")
        .expect("script closes")
        .0;
    let rules: serde_json::Value = serde_json::from_str(rules).expect("valid JSON");
    assert_eq!(rules["prefetch"][0]["eagerness"], "eager");
    assert_eq!(rules["prerender"][0]["eagerness"], "conservative");
    assert_eq!(rules["prefetch"][0]["where"]["href_matches"], "/*");
    assert!(!site.exists("public/spa.js"), "nothing is shipped");
}

/// Neither output exists unless the site asks for it.
#[test]
fn nothing_is_emitted_without_the_blocks() {
    let site = Site::with(
        r#"
        site "T"
        paths { content "content"; dist "public" }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nHi.\n",
    );
    site.stats();

    assert!(!site.exists("public/site.html"));
    assert!(!site.exists("public/spa.js"));
}
