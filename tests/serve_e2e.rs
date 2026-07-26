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
    // A site emitting a `404.html` gets it served for unmatched URLs (with
    // the 404 status intact) matching what a static host would do.
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

    // Watching -> the SSE client is injected.
    let srv = Serve::start(&t, &[]);
    let (_, body) = srv.get("/");
    assert!(
        body.contains("EventSource"),
        "reload client missing: {body}"
    );
    drop(srv);

    // --no-watch -> no injection.
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

    let mut n = 0;
    let pushed = awaits_reload(&srv, || {
        n += 1;
        t.write(
            "content/index.typ",
            &format!("#let frontmatter = (title: \"H\",)\nv{n}"),
        );
    });
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

    // Only the config file is edited: it sits at the project root, outside the
    // content/templates/assets watch roots, so this proves the config file
    // itself is watched and reloads the session.
    let mut n = 0;
    let pushed = awaits_reload(&srv, || {
        n += 1;
        t.write(
            "config.kdl",
            &format!(
                "site \"S{n}\"\npaths {{\n  content \"content\"\n  dist \"public\"\n}}\nserve {{ open #false; }}"
            ),
        );
    });
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

/// A `config.kdl` reload that moves `paths { dist }` must reach the request
/// thread: the handler captured the served root at startup and kept serving the
/// old directory for the rest of the session.
#[test]
fn a_config_reload_moves_the_served_root() {
    let t = Site::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nserve { open #false; }",
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nfirst dist",
    );
    let srv = Serve::start(&t, &[]);
    assert!(srv.get("/").1.contains("first dist"));

    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nsecond dist",
    );
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"other\"\n}\nserve { open #false; }",
    );

    // The rebuild is debounced; poll until the new root is being served.
    let served = (0..30).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        srv.get("/").1.contains("second dist")
    });
    assert!(served, "still serving the old dist after a config reload");
}

/// A build failure is a warning whether it is the first build or a later one:
/// the same typo used to kill the server or merely warn, depending only on
/// whether it predated `serve`.
#[test]
fn a_failing_first_build_keeps_the_server_up() {
    let t = Site::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nserve { open #false; }",
    );
    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\n#(",
    );

    let srv = Serve::start(&t, &[]);
    // Nothing built, so the request 404s; the point is that it answers at all.
    assert_eq!(srv.get("/").0, 404);

    t.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nfixed",
    );
    let served = (0..30).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        srv.get("/").1.contains("fixed")
    });
    assert!(served, "server did not recover once the page was fixed");
}

/// Slugs keep Unicode letters, and a browser percent-encodes every non-ASCII
/// byte, so without decoding the request path every such page 404s in `serve`.
#[test]
fn a_unicode_url_is_served() {
    let t = Site::new();
    t.write(
        "config.kdl",
        "site \"S\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nserve { open #false; }",
    );
    t.write(
        "content/posts/café.typ",
        "#let frontmatter = (title: \"Café\",)\nfound me",
    );
    let srv = Serve::start(&t, &["--no-watch"]);

    let (code, body) = srv.get_raw("/posts/caf%C3%A9/");
    assert_eq!(code, 200, "percent-encoded: {body}");
    assert!(body.contains("found me"), "{body}");
    // Traversal is still refused after decoding.
    assert_eq!(srv.get_raw("/%2e%2e/config.kdl").0, 404);
}

/// Open the live-reload stream, then apply `trigger` until a reload arrives.
///
/// Repeating the edit rather than sleeping once before it removes the race
/// between the SSE client connecting and the change landing, which a loaded
/// runner loses.
fn awaits_reload(srv: &Serve, mut trigger: impl FnMut()) -> bool {
    let mut stream = Command::new("curl")
        .args([
            "-s",
            "-N",
            "--max-time",
            "20",
            &format!("http://127.0.0.1:{}/__baudelaire/live", srv.port()),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("curl");

    let reader = BufReader::new(stream.stdout.take().expect("piped stdout"));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("data: reload") {
                let _ = tx.send(());
                return;
            }
        }
    });

    let seen = (0..40).any(|_| {
        trigger();
        rx.recv_timeout(Duration::from_millis(250)).is_ok()
    });
    let _ = stream.kill();
    let _ = stream.wait();
    seen
}
