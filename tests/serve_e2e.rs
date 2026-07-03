use std::fs;
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

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

    fn write(&self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn run(&self, args: &[&str]) -> Serve {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_baudelaire"));
        cmd.args(args).current_dir(&self.root);
        Serve(cmd.spawn().expect("spawn binary"))
    }
}

/// A spawned `serve` process, killed and reaped on drop so an early panic
/// never leaks a child holding its port.
struct Serve(std::process::Child);

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_for_server(port: u16, timeout_ms: u64) {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("server did not start within {timeout_ms}ms");
}

fn get(port: u16, path: &str) -> (u16, String) {
    let resp = std::process::Command::new("curl")
        .args(["-s", "-o", "-", "-w", "\n%{http_code}", &format!("http://127.0.0.1:{port}{path}")])
        .output()
        .expect("curl");
    let out = String::from_utf8_lossy(&resp.stdout);
    let mut lines = out.lines().rev();
    let code: u16 = lines.next().unwrap_or("0").parse().unwrap_or(0);
    let body = out[..out.len().saturating_sub(code.to_string().len() - 1)].to_string();
    (code, body)
}

#[test]
fn serve_responds_with_page() {
    let t = Tmp::new();
    t.write(
        "config.kdl",
        r#"
            site "Serve"
            paths {
                content "content"
                dist "public"
            }
            clean #true
            serve { open #false; }
        "#,
    );
    t.write("content/posts/hello.typ", "#frontmatter((title: \"Hi\",))\nHello body");
    let port = free_port();
    let _srv = t.run(&["serve", "--no-watch", "--port", &port.to_string()]);
    wait_for_server(port, 5000);
    let (code, body) = get(port, "/posts/hello/");
    assert_eq!(code, 200);
    assert!(body.contains("Hello body"));
}

#[test]
fn serve_404_for_missing() {
    let t = Tmp::new();
    t.write("config.kdl", r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        serve { open #false; }"#);
    t.write("content/index.typ", "#frontmatter((title: \"H\",))\nhome");
    let port = free_port();
    let _srv = t.run(&["serve", "--no-watch", "--port", &port.to_string()]);
    wait_for_server(port, 5000);
    let (code, _) = get(port, "/nonexistent");
    assert_eq!(code, 404);
}

#[test]
fn serve_resolves_without_trailing_slash() {
    let t = Tmp::new();
    t.write("config.kdl", r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        clean #true
        serve { open #false; }"#);
    t.write("content/posts/hello.typ", "#frontmatter((title: \"Hi\",))\nbody");
    let port = free_port();
    let _srv = t.run(&["serve", "--no-watch", "--port", &port.to_string()]);
    wait_for_server(port, 5000);
    let (code, _) = get(port, "/posts/hello");
    assert_eq!(code, 200);
    let (code, _) = get(port, "/posts/hello/");
    assert_eq!(code, 200);
}

#[test]
fn live_reload_script_injected_only_when_watching() {
    let t = Tmp::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\nserve { open #false; }",
    );
    t.write("content/index.typ", "#frontmatter((title: \"H\",))\nhome");

    // Watching → the SSE client is injected.
    let port = free_port();
    let srv = t.run(&["serve", "--port", &port.to_string()]);
    wait_for_server(port, 5000);
    let (_, body) = get(port, "/");
    assert!(body.contains("EventSource"), "reload client missing: {body}");
    drop(srv);

    // --no-watch → no injection.
    let port = free_port();
    let _srv = t.run(&["serve", "--no-watch", "--port", &port.to_string()]);
    wait_for_server(port, 5000);
    let (_, body) = get(port, "/");
    assert!(!body.contains("EventSource"), "reload client should be absent: {body}");
}

#[test]
fn sse_stream_pushes_reload_on_change() {
    let t = Tmp::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nclean #true\nserve { open #false; }",
    );
    t.write("content/index.typ", "#frontmatter((title: \"H\",))\nv1");
    let port = free_port();
    let _srv = t.run(&["serve", "--port", &port.to_string()]);
    wait_for_server(port, 5000);

    // Open the event stream in the background, capped so it can't hang.
    let stream = Command::new("curl")
        .args([
            "-s",
            "-N",
            "--max-time",
            "5",
            &format!("http://127.0.0.1:{port}/__baudelaire/live"),
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("curl");

    // Give the stream a moment to connect, then trigger a rebuild.
    std::thread::sleep(Duration::from_millis(800));
    t.write("content/index.typ", "#frontmatter((title: \"H\",))\nv2");

    let out = stream.wait_with_output().expect("curl output");
    let body = String::from_utf8_lossy(&out.stdout);
    assert!(body.contains("data: reload"), "no reload event pushed: {body:?}");
}

#[test]
fn serve_serves_index_at_root() {
    let t = Tmp::new();
    t.write("config.kdl", r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        clean #true
        serve { open #false; }"#);
    t.write("content/index.typ", "#frontmatter((title: \"Home\",))\nwelcome home");
    let port = free_port();
    let _srv = t.run(&["serve", "--no-watch", "--port", &port.to_string()]);
    wait_for_server(port, 5000);
    let (code, body) = get(port, "/");
    assert_eq!(code, 200);
    assert!(body.contains("welcome home"));
}
