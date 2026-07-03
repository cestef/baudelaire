use std::fs;
use std::process::Command;

struct Tmp {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        Self { _tmp: tmp, root }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_baudelaire"));
        cmd.args(args).current_dir(&self.root);
        cmd.output().expect("run binary")
    }

    fn exists(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).unwrap()
    }
}

#[test]
fn init_creates_project_skeleton() {
    let t = Tmp::new();
    let out = t.run(&["init", "-r", t.root.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.exists("config.kdl"));
    assert!(t.exists("content/index.typ"));
    assert!(t.exists("content/posts/hello.typ"));
    assert!(t.exists("templates/post.typ"));
    assert!(t.exists("assets"));
}

#[test]
fn init_config_is_valid() {
    let t = Tmp::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let cfg = t.read("config.kdl");
    assert!(cfg.contains("site \"My Site\""));
    assert!(cfg.contains("clean #true"));
    assert!(cfg.contains("collections"));
}

#[test]
fn init_then_build_works() {
    let t = Tmp::new();
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
    let t = Tmp::new();
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
    assert!(body.contains("title: \"Untitled\""));
    assert!(body.contains("draft: true"));
}

#[test]
fn new_refuses_existing_file() {
    let t = Tmp::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["new", "content/posts/hello.typ"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("already exists"));
}

#[test]
fn new_creates_parent_dirs() {
    let t = Tmp::new();
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
    let t = Tmp::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["-v", "build"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("built"));
}

#[test]
fn quiet_suppresses_milestone() {
    let t = Tmp::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["-q", "build"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("building"));
}

#[test]
fn build_reports_timing() {
    let t = Tmp::new();
    t.run(&["init", "-r", t.root.to_str().unwrap()]);
    let out = t.run(&["build"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ms") || stdout.contains("s)"));
}
