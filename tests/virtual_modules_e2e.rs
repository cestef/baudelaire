//! The `baudelaire:*` virtual JS modules: a user entry imports one and rolldown
//! inlines the generated data into the bundle.

mod common;

use baudelaire::graph::Hash;
use common::Site;

/// The 16-hex fingerprint baudelaire splices into an asset's filename.
fn fingerprint(bytes: &[u8]) -> String {
    Hash::of_bytes(bytes).hex()[..16].to_owned()
}

fn build(site: &Site) {
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build `site` (fingerprinting off) and return its bundled entry, read at its
/// stable name.
fn bundle(site: &Site) -> String {
    build(site);
    site.read("public/assets/main.js")
}

fn config(extra: &str) -> String {
    format!(
        "site \"T\"\nurl \"https://ex.com\"\nauthor \"Ada\"\npaths {{\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}}\nclean #true\n{extra}\n"
    )
}

#[test]
fn site_module_exposes_identity_and_version() {
    let site = Site::new();
    site.write("config.kdl", &config("output { assets { bundle #true } }"));
    site.write(
        "assets/main.js",
        "import site from \"baudelaire:site\";\nglobalThis.S = site;\n",
    );
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nx");
    let js = bundle(&site);
    assert!(js.contains("https://ex.com"), "url inlined: {js}");
    assert!(js.contains("Ada"), "author inlined: {js}");
    assert!(!js.contains("import "), "specifier resolved away: {js}");
}

#[test]
fn config_module_exposes_client_constants() {
    let site = Site::new();
    site.write(
        "config.kdl",
        &config("client {\n  env \"prod\"\n  retries 3\n  beta #true\n}\noutput { assets { bundle #true } }"),
    );
    site.write(
        "assets/main.js",
        "import cfg from \"baudelaire:config\";\nglobalThis.C = cfg;\n",
    );
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nx");
    let js = bundle(&site);
    assert!(js.contains("prod"), "string constant inlined: {js}");
    assert!(js.contains('3'), "number constant inlined: {js}");
}

#[test]
fn assets_module_resolves_fingerprinted_urls() {
    let site = Site::new();
    site.write(
        "config.kdl",
        &config("output { assets { bundle #true; fingerprint #true } }"),
    );
    let logo = b"<svg></svg>";
    site.write("assets/logo.svg", std::str::from_utf8(logo).unwrap());
    site.write(
        "assets/main.js",
        "import { url } from \"baudelaire:assets\";\nglobalThis.L = url(\"/assets/logo.svg\");\n",
    );
    site.write("content/a.typ", "#let frontmatter = (title: \"A\",)\nx");
    build(&site);
    // The logo's fingerprinted name is derived from its bytes, not discovered
    // by scanning the output directory.
    let hashed = format!("logo.{}.svg", fingerprint(logo));
    assert!(
        site.exists(&format!("public/assets/{hashed}")),
        "logo fingerprinted to {hashed}"
    );
    // The bundled entry (the sole `.js`, its own name fingerprinted by the
    // unpredictable bundle bytes) must carry the logo's hashed name in its map.
    let js = entry(&site, ".js");
    assert!(js.contains(&hashed), "map serves the hashed name: {js}");
}

#[test]
fn pages_module_lists_authored_content() {
    let site = Site::new();
    site.write(
        "config.kdl",
        &config("taxonomies { tags }\noutput { assets { bundle #true } }"),
    );
    site.write(
        "assets/main.js",
        "import pages from \"baudelaire:pages\";\nglobalThis.P = pages;\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello Post\", date: datetime(year: 2026, month: 7, day: 14), tags: (\"rust\",))\nx",
    );
    let js = bundle(&site);
    assert!(js.contains("Hello Post"), "title inlined: {js}");
    assert!(js.contains("2026-07-14"), "iso date inlined: {js}");
    assert!(js.contains("rust"), "taxonomy inlined: {js}");
}

#[test]
fn sections_module_groups_by_collection() {
    let site = Site::new();
    site.write("config.kdl", &config("output { assets { bundle #true } }"));
    site.write(
        "assets/main.js",
        "import sections from \"baudelaire:sections\";\nglobalThis.N = sections;\n",
    );
    site.write(
        "content/posts/one.typ",
        "#let frontmatter = (title: \"One\",)\nx",
    );
    let js = bundle(&site);
    assert!(js.contains("posts"), "collection id inlined: {js}");
    assert!(js.contains("One"), "page title inlined: {js}");
}

#[test]
fn taxonomies_module_maps_terms_to_pages() {
    let site = Site::new();
    site.write(
        "config.kdl",
        &config("taxonomies { tags }\noutput { assets { bundle #true } }"),
    );
    site.write(
        "assets/main.js",
        "import taxonomies from \"baudelaire:taxonomies\";\nglobalThis.T = taxonomies;\n",
    );
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"Alpha\", tags: (\"rust\", \"web\"))\nx",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"Beta\", tags: (\"rust\",))\nx",
    );
    let js = bundle(&site);
    assert!(js.contains("rust"), "term inlined: {js}");
    assert!(
        js.contains("Alpha") && js.contains("Beta"),
        "pages inlined: {js}"
    );
}

#[test]
fn feed_module_lists_recent_dated_pages_newest_first() {
    let site = Site::new();
    site.write("config.kdl", &config("output { assets { bundle #true } }"));
    site.write(
        "assets/main.js",
        "import feed from \"baudelaire:feed\";\nglobalThis.F = feed;\n",
    );
    site.write(
        "content/posts/old.typ",
        "#let frontmatter = (title: \"Older\", date: datetime(year: 2024, month: 1, day: 1))\nx",
    );
    site.write(
        "content/posts/new.typ",
        "#let frontmatter = (title: \"Newer\", date: datetime(year: 2026, month: 6, day: 1))\nx",
    );
    let js = bundle(&site);
    let newer = js.find("Newer").expect("newer listed");
    let older = js.find("Older").expect("older listed");
    assert!(newer < older, "newest first: {js}");
}

/// The sole emitted file under `public/assets` with extension `ext`. Read by
/// uniqueness, not by guessing its fingerprint — the entry's hash depends on the
/// bundler's output, which the test can't reproduce.
fn entry(site: &Site, ext: &str) -> String {
    let mut found: Vec<_> = site
        .files("public/assets")
        .into_iter()
        .filter(|f| f.ends_with(ext))
        .collect();
    assert_eq!(found.len(), 1, "exactly one {ext} file: {found:?}");
    site.read(&format!("public/assets/{}", found.remove(0)))
}
