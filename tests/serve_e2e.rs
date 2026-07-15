mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use common::{Serve, Site};

#[test]
fn serve_responds_with_page() {
    let t = Site::new();
    t.write(
        "config.kdl",
        r#"
            site "Serve"
            paths {
                content "content"
                dist "public"
            }
            serve { open #false; }
        "#,
    );
    t.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hi\",)\nHello body",
    );
    let srv = Serve::start(&t, &["--no-watch"]);
    let (code, body) = srv.get("/posts/hello/");
    assert_eq!(code, 200);
    assert!(body.contains("Hello body"));
}

#[test]
fn serve_rejects_path_traversal() {
    let t = Site::new();
    t.write(
        "config.kdl",
        r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        serve { open #false; }"#,
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome",
    );
    // A secret sibling of `dist`, inside the project root but outside the
    // served tree. `config.kdl` itself is such a file.
    let srv = Serve::start(&t, &["--no-watch"]);
    for attack in ["/../config.kdl", "/../../etc/hostname", "/..%2fconfig.kdl"] {
        let (code, body) = srv.get_raw(attack);
        assert_eq!(code, 404, "traversal {attack} not blocked: {body}");
        assert!(!body.contains("dist"), "leaked config via {attack}: {body}");
    }
}

#[test]
fn serve_404_for_missing() {
    let t = Site::new();
    t.write(
        "config.kdl",
        r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        serve { open #false; }"#,
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome",
    );
    let srv = Serve::start(&t, &["--no-watch"]);
    let (code, _) = srv.get("/nonexistent");
    assert_eq!(code, 404);
}

#[test]
fn serve_falls_back_to_the_site_404_page() {
    // A site emitting a `404.html` gets it served for unmatched URLs — with
    // the 404 status intact — matching what a static host would do.
    let t = Site::new();
    t.write(
        "config.kdl",
        r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        serve { open #false; }"#,
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome",
    );
    t.write(
        "content/404.typ",
        "#let frontmatter = (title: \"Nope\",)\nnothing here, friend",
    );
    let srv = Serve::start(&t, &["--no-watch"]);
    let (code, body) = srv.get("/nonexistent");
    assert_eq!(code, 404);
    assert!(body.contains("nothing here, friend"), "{body}");
}

#[test]
fn serve_resolves_without_trailing_slash() {
    let t = Site::new();
    t.write(
        "config.kdl",
        r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        serve { open #false; }"#,
    );
    t.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hi\",)\nbody",
    );
    let srv = Serve::start(&t, &["--no-watch"]);
    let (code, _) = srv.get("/posts/hello");
    assert_eq!(code, 200);
    let (code, _) = srv.get("/posts/hello/");
    assert_eq!(code, 200);
}

#[test]
fn live_reload_script_injected_only_when_watching() {
    let t = Site::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nserve { open #false; }",
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome",
    );

    // Watching → the SSE client is injected.
    let srv = Serve::start(&t, &[]);
    let (_, body) = srv.get("/");
    assert!(
        body.contains("EventSource"),
        "reload client missing: {body}"
    );
    drop(srv);

    // --no-watch → no injection.
    let srv = Serve::start(&t, &["--no-watch"]);
    let (_, body) = srv.get("/");
    assert!(
        !body.contains("EventSource"),
        "reload client should be absent: {body}"
    );
}

#[test]
fn sse_stream_pushes_reload_on_change() {
    let t = Site::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nserve { open #false; }",
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nv1",
    );
    let srv = Serve::start(&t, &[]);

    // Open the event stream in the background. `--max-time` only guards against
    // a hang: on success the read below exits the instant the event arrives.
    let mut stream = Command::new("curl")
        .args([
            "-s",
            "-N",
            "--max-time",
            "10",
            &format!("http://127.0.0.1:{}/__baudelaire/live", srv.port()),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("curl");

    // Give the stream a moment to connect, then trigger a rebuild.
    std::thread::sleep(Duration::from_millis(200));
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nv2",
    );

    // Read until the reload event, then stop — don't wait out the whole stream
    // (an SSE connection stays open, so `wait_with_output` would block for the
    // full `--max-time`).
    let reader = BufReader::new(stream.stdout.take().expect("piped stdout"));
    let pushed = reader
        .lines()
        .map_while(Result::ok)
        .any(|line| line.contains("data: reload"));
    let _ = stream.kill();
    let _ = stream.wait();
    assert!(pushed, "no reload event pushed");
}

#[test]
fn config_edit_reloads_and_pushes_reload() {
    let t = Site::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nserve { open #false; }",
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome",
    );
    let srv = Serve::start(&t, &[]);

    // Open the event stream, then edit only the config file — it sits at the
    // project root, outside the content/templates/assets watch roots, so this
    // proves the config file itself is watched and reloads the session.
    let mut stream = Command::new("curl")
        .args([
            "-s",
            "-N",
            "--max-time",
            "10",
            &format!("http://127.0.0.1:{}/__baudelaire/live", srv.port()),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("curl");
    std::thread::sleep(Duration::from_millis(200));
    t.write(
        "config.kdl",
        "site \"S2\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nserve { open #false; }",
    );

    let reader = BufReader::new(stream.stdout.take().expect("piped stdout"));
    let pushed = reader
        .lines()
        .map_while(Result::ok)
        .any(|line| line.contains("data: reload"));
    let _ = stream.kill();
    let _ = stream.wait();
    assert!(pushed, "config edit did not trigger a rebuild + reload");
}

#[test]
fn serve_serves_index_at_root() {
    let t = Site::new();
    t.write(
        "config.kdl",
        r#"site "S"
        paths {
            content "content"
            dist "public"
        }
        serve { open #false; }"#,
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nwelcome home",
    );
    let srv = Serve::start(&t, &["--no-watch"]);
    let (code, body) = srv.get("/");
    assert_eq!(code, 200);
    assert!(body.contains("welcome home"));
}
