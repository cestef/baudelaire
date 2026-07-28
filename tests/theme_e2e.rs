//! Themes: templates, assets, static files, and config defaults a site inherits
//! and overrides file by file.

mod common;

use baudelaire::config::Config;
use common::Site;

/// A site with a theme in `themes/plume`, carrying a template, a stylesheet, a
/// static file, and config defaults.
fn site() -> Site {
    let site = Site::with(
        r#"
        site "T"
        theme "themes/plume"
        paths { content "content"; dist "public"; assets "assets"; static "static" }
        "#,
    );
    site.write(
        "themes/plume/theme.kdl",
        "site \"Theme default\"\nlang \"fr\"\nhtml {\n  pretty #false\n}\n",
    );
    site.write(
        "themes/plume/templates/page.typ",
        "#let page(data, body) = html.elem(\"main\")[#body]\n",
    );
    site.write("themes/plume/assets/theme.css", "body { color: red }\n");
    site.write("themes/plume/assets/shared.css", "body { margin: 0 }\n");
    site.write("themes/plume/static/robots.txt", "theme\n");
    site
}

/// The theme supplies the layout: the project has no `templates/` at all, and
/// the page still gets wrapped.
#[test]
fn a_page_uses_the_themes_template() {
    let site = site();
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHello.\n",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("<main>"), "theme template applied: {html}");
    assert!(html.contains("Hello."), "{html}");
}

/// A template the project has shadows the theme's, without renaming anything.
#[test]
fn a_project_template_overrides_the_themes() {
    let site = site();
    site.write(
        "templates/page.typ",
        "#let page(data, body) = html.elem(\"article\")[#body]\n",
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHello.\n",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("<article>"), "project template wins: {html}");
    assert!(!html.contains("<main>"), "{html}");
}

/// Assets and static files layer the same way: the theme's come through, and
/// the project replaces the ones it also has.
#[test]
fn assets_and_static_files_layer_with_the_project_on_top() {
    let site = site();
    site.write("assets/shared.css", "body { margin: 8px }\n");
    site.write("assets/own.css", "body { padding: 0 }\n");
    site.write("static/humans.txt", "project\n");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nHello.\n",
    );
    site.stats();

    // Only in the theme.
    assert!(site.output("assets/theme.css").contains("red"));
    assert!(site.output("robots.txt").contains("theme"));
    // In both: the project's content is what is served.
    assert!(site.output("assets/shared.css").contains("8px"));
    // Only in the project.
    assert!(site.output("assets/own.css").contains("padding"));
    assert!(site.output("humans.txt").contains("project"));
}

/// `theme.kdl` is a floor, not a ceiling: keys the site states win, keys it
/// leaves out fall back to the theme's.
#[test]
fn theme_config_supplies_defaults_the_site_overrides() {
    let site = site();
    let config = Config::load(&site.read("config.kdl"), &site.root).expect("config");

    // Stated by the site: the site wins.
    assert_eq!(config.site.as_deref(), Some("T"));
    // Stated only by the theme: inherited, nested keys included.
    assert_eq!(config.lang, "fr");
    assert!(!config.html.pretty, "nested theme default inherited");
}

/// A theme naming a directory that is not there fails at load, naming the
/// value that is wrong.
#[test]
fn a_missing_theme_is_a_precise_error() {
    let site = Site::with("site \"T\"\ntheme \"themes/absent\"\n");
    let err = Config::load(&site.read("config.kdl"), &site.root).expect_err("missing theme");
    assert!(format!("{err}").contains("themes/absent"), "{err}");
}
