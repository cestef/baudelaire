//! A page's body taken from a file above the project root.
//!
//! The case `tests/scenarios/sources.kdl` cannot state, because a scenario's
//! files are written under the one root it builds from. It is also the case the
//! feature exists for: a repository whose site lives in `docs/` publishes the
//! `CHANGELOG.md` at its top, and copying the file in was the alternative.
//!
//! Climbing out is the config's to allow, not a page's. `paths` is one of the
//! sections a theme is refused, and a page names a declared *name* rather than a
//! path, so the only thing that can point at a file up there is the site's own
//! `config.kdl` -- which can already run any command through `hooks`.

mod common;

#[cfg(feature = "markdown")]
use baudelaire::config::Config;
#[cfg(feature = "markdown")]
use common::Site;

/// A site in `proj/` whose config declares the `CHANGELOG.md` beside it, with
/// `page` as the stub that names it.
///
/// Gated with its callers: without the `markdown` feature a `.md` file is not a
/// page, so there is no body for a source to replace.
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

/// The declared path resolves against the project root and is allowed to leave
/// it, so the page's body is the file one directory up.
#[test]
#[cfg(feature = "markdown")]
fn a_source_may_sit_above_the_project_root() {
    let (site, config) = site(";;;\ntitle \"Changelog\"\nsource \"changelog\"\n;;;\n");

    let page =
        common::load_page("", &site.root.join("proj/content/changelog.md"), &config).expect("load");

    assert_eq!(page.frontmatter.title.as_deref(), Some("Changelog"));
    assert!(page.body.contains("today"), "{}", page.body);
    // Lowered as markdown, not carried through as raw text.
    assert!(page.body.contains("strong"), "{}", page.body);
}

/// Editing the sourced file changes the page, with nothing to regenerate in
/// between: it is read on every load, which is what replaces the copy that had
/// to be kept in sync.
#[test]
#[cfg(feature = "markdown")]
fn the_page_follows_the_file_it_sources() {
    let (site, config) = site(";;;\ntitle \"Changelog\"\nsource \"changelog\"\n;;;\n");
    let load = || {
        common::load_page("", &site.root.join("proj/content/changelog.md"), &config).expect("load")
    };

    assert!(load().body.contains("today"));
    site.write("CHANGELOG.md", "Released *yesterday*.\n");
    assert!(load().body.contains("yesterday"));
}
