use std::fs;
use std::process::Command;

use baudelaire::config::Config;
use baudelaire::content::discover;

struct Sandbox {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        Self { _tmp: tmp, root }
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn config(&self) -> Config {
        let text = fs::read_to_string(self.root.join("config.kdl")).unwrap();
        let mut cfg = Config::parse(&text).unwrap();
        cfg.content = self.root.join(&cfg.content);
        cfg.dist = self.root.join(&cfg.dist);
        cfg.cache.dir = self.root.join(&cfg.cache.dir);
        cfg
    }
}

fn bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_baudelaire").into()
}

fn run_in(sb: &Sandbox, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(&sb.root);
    cmd.output().expect("run binary")
}

/// Spawn a long-running command (e.g. `serve`), reaped on drop.
fn spawn_in(sb: &Sandbox, args: &[&str]) -> Child {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(&sb.root);
    Child(cmd.spawn().expect("spawn binary"))
}

/// A spawned child process, killed and reaped on drop.
struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Poll until `port` accepts a connection, up to `timeout_ms`.
fn wait_for_port(port: u16, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

#[test]
fn root_flag_builds_from_that_directory() {
    let sb = Sandbox::new();
    // The site lives in a subdirectory; we invoke from the sandbox root.
    sb.write(
        "site/config.kdl",
        "site \"R\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\n",
    );
    sb.write("site/content/index.typ", "#frontmatter((title: \"H\",))\nhome");
    let out = run_in(&sb, &["--root", "site", "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sb.root.join("site/public/index.html").exists());
    // Nothing leaked into the invocation cwd.
    assert!(!sb.root.join("public").exists());
}

#[test]
fn clean_removes_dist_and_cache() {
    let sb = Sandbox::new();
    sb.write("config.kdl", "site \"T\"\npaths {\n  dist \"public\"\n}\n");
    sb.write("public/index.html", "<html></html>");
    sb.write(".baudelaire/cache/hashes.json", "{}");
    let cfg = sb.config();
    assert!(cfg.dist.exists());
    assert!(cfg.cache.dir.exists());
    // call the clean logic directly via the CLI
    let out = run_in(
        &sb,
        &["-c", sb.root.join("config.kdl").to_str().unwrap(), "clean"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!cfg.dist.exists(), "dist still exists");
    assert!(!cfg.cache.dir.exists(), "cache still exists");
}

#[test]
fn clean_idempotent_when_dirs_absent() {
    let sb = Sandbox::new();
    sb.write("config.kdl", "site \"T\"\npaths {\n  dist \"public\"\n}\n");
    let cfg = sb.config();
    assert!(!cfg.dist.exists());
    let out = run_in(
        &sb,
        &["-c", sb.root.join("config.kdl").to_str().unwrap(), "clean"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!cfg.dist.exists());
}

#[test]
fn missing_config_reports_not_found() {
    let sb = Sandbox::new();
    let out = run_in(
        &sb,
        &["-c", sb.root.join("nope.kdl").to_str().unwrap(), "build"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("config file not found") || stderr.contains("not_found"));
}

#[test]
fn config_collection_lookup() {
    let sb = Sandbox::new();
    sb.write(
        "config.kdl",
        r#"
            site "T"
            collections {
              posts "posts/**/*.typ" sort="date"
              notes "notes/**/*.typ"
            }
        "#,
    );
    let cfg = sb.config();
    let posts = cfg.collection("posts").expect("posts exists");
    assert_eq!(posts.sort, baudelaire::config::SortKey::Date);
    assert!(cfg.collection("missing").is_none());
}

#[test]
fn default_build_works_no_content() {
    let sb = Sandbox::new();
    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}",
    );
    let out = run_in(
        &sb,
        &["-c", sb.root.join("config.kdl").to_str().unwrap(), "build"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("built") || stdout.contains("building"));
}

#[test]
fn profile_flag_applied() {
    // `serve` blocks, so it can't be run with `output()`. Spawn it, prove the
    // profile's port override took effect by connecting to it, then reap.
    let port = free_port();
    let sb = Sandbox::new();
    sb.write(
        "config.kdl",
        &format!(
            r#"
            site "T"
            serve {{ port 3000; open #false; }}
            profiles {{
              ci {{
                serve {{ port {port}; }}
              }}
            }}
        "#
        ),
    );
    let _srv = spawn_in(
        &sb,
        &[
            "-c",
            sb.root.join("config.kdl").to_str().unwrap(),
            "--profile",
            "ci",
            "serve",
            "--no-watch",
        ],
    );
    // Default port is 3000; reachability on `port` proves the `ci` profile won.
    assert!(
        wait_for_port(port, 5000),
        "server never bound profile port {port}"
    );
}

#[test]
fn discover_with_collection_override() {
    let sb = Sandbox::new();
    sb.write(
        "config.kdl",
        r#"
            site "T"
            paths {
                content "content"
                dist "public"
            }
            collections {
              posts permalink="/blog/{slug}/"
            }
        "#,
    );
    sb.write(
        "content/posts/hello.typ",
        "#frontmatter((title: \"Hi\",))\nbody",
    );
    let cfg = sb.config();
    let cols = discover(&cfg).unwrap();
    let posts = cols.iter().find(|c| c.id == "posts").unwrap();
    assert_eq!(posts.pages.len(), 1);
    assert_eq!(posts.pages[0].permalink, "/blog/hello/");
}
