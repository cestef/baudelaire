mod common;

use common::Site;

#[test]
fn init_creates_project_skeleton() {
    let t = Site::new();
    let out = t.run(&["init", "-r", t.root.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.exists("config.kdl"));
    assert!(t.exists("content/index.typ"));
    assert!(t.exists("content/posts/hello.typ"));
    assert!(t.exists("templates/layout.typ"));
    assert!(t.exists("assets/style.css"));
}

#[test]
fn init_config_is_valid() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let cfg = t.read("config.kdl");
    // The site name is templated from the directory, and every placeholder is
    // filled (no `{{..}}` left behind).
    assert!(cfg.contains("site \""), "has a site name: {cfg}");
    assert!(!cfg.contains("{{"), "placeholders filled: {cfg}");
    assert!(cfg.contains("clean #true"));
    assert!(cfg.contains("collections"));
}

#[test]
fn init_then_build_works() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.exists("public/posts/hello-world/index.html"));
}

#[test]
fn new_scaffolds_content_file() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["new", "content/posts/my-post.typ"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.exists("content/posts/my-post.typ"));
    let body = t.read("content/posts/my-post.typ");
    assert!(body.contains("frontmatter"));
    // Title is derived from the filename, not a fixed "Untitled".
    assert!(body.contains("title: \"My Post\""), "{body}");
    assert!(body.contains("draft: true"));
    // The permalink the page will occupy is reported.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("/posts/my-post/"),
        "permalink preview: {stderr}"
    );
}

#[test]
fn new_infers_frontmatter_from_the_collection() {
    let t = Site::new();
    t.write(
        "config.kdl",
        "site \"T\"\ncollections {\n  blog sort=\"date\"\n  guide sort=\"order\"\n}\n",
    );
    // A dated collection gets today's date stamped; no order field.
    let blog = t.run(&["new", "blog/launch-day"]);
    assert!(
        blog.status.success(),
        "{}",
        String::from_utf8_lossy(&blog.stderr)
    );
    let body = t.read("content/blog/launch-day.typ");
    assert!(body.contains("title: \"Launch Day\""), "{body}");
    assert!(
        body.contains("date: datetime("),
        "dated collection stamps a date: {body}"
    );
    assert!(
        !body.contains("order:"),
        "dated collection has no order: {body}"
    );

    // An ordered collection gets the next order; first page is 1.
    t.run(&["new", "guide/intro"]);
    assert!(t.read("content/guide/intro.typ").contains("order: 1"));
    t.run(&["new", "guide/second"]);
    assert!(
        t.read("content/guide/second.typ").contains("order: 2"),
        "order increments past existing pages"
    );
}

#[test]
fn new_bundle_creates_index_in_a_directory() {
    let t = Site::new();
    t.write("config.kdl", "site \"T\"\n");
    let out = t.run(&["new", "posts/my-post", "--bundle"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        t.exists("content/posts/my-post/index.typ"),
        "bundle dir with index.typ"
    );
    // The bundle takes its title from the directory, not "Index".
    assert!(t
        .read("content/posts/my-post/index.typ")
        .contains("title: \"My Post\""));
}

#[test]
fn new_refuses_existing_file() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["new", "content/posts/hello.typ"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("already exists"));
}

#[test]
fn new_creates_parent_dirs() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["new", "content/posts/deep/nested/post.typ"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.exists("content/posts/deep/nested/post.typ"));
}

#[test]
fn verbose_shows_per_page_progress() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["-v", "build"]);
    assert!(out.status.success());
    let logs = String::from_utf8_lossy(&out.stderr);
    assert!(logs.contains("built"));
}

#[test]
fn quiet_suppresses_milestone() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["-q", "build"]);
    assert!(out.status.success());
    let logs = String::from_utf8_lossy(&out.stderr);
    assert!(!logs.contains("building"));
}

#[test]
fn build_reports_timing() {
    let t = Site::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["build"]);
    assert!(out.status.success());
    // The summary ends `.. in 132ms` / `.. in 1.24s`.
    let logs = String::from_utf8_lossy(&out.stderr);
    assert!(
        logs.contains(" in ") && (logs.contains("ms") || logs.contains("s")),
        "{logs}"
    );
}
