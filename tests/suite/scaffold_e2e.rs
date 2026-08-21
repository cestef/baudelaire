use crate::common::Site;

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
    assert!(cfg.contains("site \""), "has a site name: {cfg}");
    assert!(!cfg.contains("{{"), "placeholders filled: {cfg}");
    assert!(cfg.contains("prune #true"));
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
    assert!(body.contains("title: \"My Post\""), "{body}");
    assert!(body.contains("draft: true"));
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
        "site \"T\"\ncontent {\n  collections {\n    blog { sort \"date\" }\n    guide { sort \"order\" }\n  }\n}\n",
    );
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
    assert!(
        t.read("content/posts/my-post/index.typ")
            .contains("title: \"My Post\"")
    );
}

/// A bundle's name is a directory name, and a directory may hold a dot.
#[test]
fn new_bundle_keeps_a_dot_in_its_name() {
    let t = Site::new();
    t.write("config.kdl", "site \"T\"\n");
    let out = t.run(&["new", "posts/v1.2", "--bundle"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        t.exists("content/posts/v1.2/index.typ"),
        "bundle kept its dot"
    );
    assert!(
        !t.exists("content/posts/v1/index.typ"),
        "truncated at the dot"
    );
    let out = t.run(&["new", "posts/other.typ", "--bundle"]);
    assert!(out.status.success());
    assert!(t.exists("content/posts/other/index.typ"));
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
    let verbose = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(verbose.contains("index.typ"), "no per-page line: {verbose}");

    let quiet = t.run(&["build"]);
    let quiet = String::from_utf8_lossy(&quiet.stderr).into_owned();
    assert!(
        !quiet.contains("index.typ"),
        "per-page lines are not verbose-only: {quiet}"
    );
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
        logs.split(" in ").skip(1).any(|tail| {
            let unit = tail.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
            tail.starts_with(|c: char| c.is_ascii_digit())
                && (unit.starts_with("ms") || unit.starts_with('s') || unit.starts_with("µs"))
        }),
        "no duration in summary: {logs}"
    );
}
