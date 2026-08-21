//! A page's body taken from a file above the project root.

#[cfg(feature = "markdown")]
use crate::common::Site;
#[cfg(feature = "markdown")]
use baudelaire::config::Config;

/// A site in `proj/` whose config declares the `CHANGELOG.md` beside it, with
/// `page` as the stub that names it.
///
/// Gated with its callers: without `markdown` a `.md` file is not a page.
#[cfg(feature = "markdown")]
fn site(page: &str) -> (Site, Config) {
    let site = Site::new();
    site.write("CHANGELOG.md", "Released **today**.\n");
    site.write(
        "proj/config.kdl",
        "site \"T\"\n\
         paths {\n  \
           content \"content\"\n  \
           dist \"public\"\n  \
           sources {\n    changelog \"../CHANGELOG.md\"\n  }\n\
         }\n",
    );
    site.write("proj/content/changelog.md", page);

    let root = site.root.join("proj");
    let mut config = Config::load(&site.read("proj/config.kdl"), &root, None).expect("config");
    config.root.clone_from(&root);
    config.paths.content = root.join("content");
    config.paths.dist = root.join("public");
    config.cache.dir = root.join(&config.cache.dir);
    (site, config)
}

#[test]
#[cfg(feature = "markdown")]
fn a_source_may_sit_above_the_project_root() {
    let (site, config) = site(";;;\ntitle \"Changelog\"\nsource \"changelog\"\n;;;\n");

    let page = crate::common::load_page("", &site.root.join("proj/content/changelog.md"), &config)
        .expect("load");

    assert_eq!(page.frontmatter.title.as_deref(), Some("Changelog"));
    assert!(page.body.contains("today"), "{}", page.body);
    assert!(page.body.contains("strong"), "{}", page.body);
}

#[test]
#[cfg(feature = "markdown")]
fn the_page_follows_the_file_it_sources() {
    let (site, config) = site(";;;\ntitle \"Changelog\"\nsource \"changelog\"\n;;;\n");
    let load = || {
        crate::common::load_page("", &site.root.join("proj/content/changelog.md"), &config)
            .expect("load")
    };

    assert!(load().body.contains("today"));
    site.write("CHANGELOG.md", "Released *yesterday*.\n");
    assert!(load().body.contains("yesterday"));
}
