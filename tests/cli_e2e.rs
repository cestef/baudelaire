mod common;

use baudelaire::content::discover;

use common::{Site, free_port, project, wait_for_port};

#[test]
fn root_flag_builds_from_that_directory() {
    let sb = Site::new();
    // The site lives in a subdirectory; we invoke from the sandbox root.
    sb.write(
        "site/config.kdl",
        "site \"R\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    sb.write(
        "site/content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome",
    );
    let out = sb.run(&["--root", "site", "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sb.exists("site/public/index.html"));
    // Nothing leaked into the invocation cwd.
    assert!(!sb.exists("public"));
}

#[test]
fn help_groups_global_flags_and_shows_examples() {
    let sb = Site::new();
    let out = sb.run(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    // Global flags cluster under concern headings rather than one long list.
    // Build-shaping flags live on the build-like subcommands, not the top level.
    for heading in ["Project:", "Logging:", "Examples:"] {
        assert!(
            help.contains(heading),
            "missing `{heading}` in help:\n{help}"
        );
    }
    // The examples block is present.
    assert!(
        help.contains("baudelaire serve --open"),
        "help shows an example invocation"
    );
    // Piped output must not leak raw ANSI escapes.
    assert!(!help.contains('\x1b'), "no escape codes when not a TTY");

    // Build-shaping overrides are grouped under `build --help`, not global help.
    let build = sb.run(&["build", "--help"]);
    assert!(build.status.success());
    let build_help = String::from_utf8_lossy(&build.stdout);
    for heading in ["Output:", "Build:"] {
        assert!(
            build_help.contains(heading),
            "missing `{heading}` in build help:\n{build_help}"
        );
    }
    assert!(
        build_help.contains("--drafts"),
        "build help lists build flags"
    );
}

#[test]
fn clean_removes_dist_and_cache() {
    let sb = Site::new();
    sb.write("config.kdl", "site \"T\"\npaths {\n  dist \"public\"\n}\n");
    sb.write("public/index.html", "<html></html>");
    sb.write(".baudelaire/cache/hashes.json", "{}");
    let cfg = sb.config();
    assert!(cfg.paths.dist.exists());
    assert!(cfg.cache.dir.exists());
    // call the clean logic directly via the CLI
    let out = sb.run(&["-c", sb.path("config.kdl").to_str().unwrap(), "clean"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!cfg.paths.dist.exists(), "dist still exists");
    assert!(!cfg.cache.dir.exists(), "cache still exists");
}

#[test]
fn clean_idempotent_when_dirs_absent() {
    let sb = Site::new();
    sb.write("config.kdl", "site \"T\"\npaths {\n  dist \"public\"\n}\n");
    let cfg = sb.config();
    assert!(!cfg.paths.dist.exists());
    let out = sb.run(&["-c", sb.path("config.kdl").to_str().unwrap(), "clean"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!cfg.paths.dist.exists());
}

#[test]
fn missing_config_reports_not_found() {
    let sb = Site::new();
    let out = sb.run(&["-c", sb.path("nope.kdl").to_str().unwrap(), "build"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("config file not found") || stderr.contains("not_found"));
}

#[test]
fn config_collection_lookup() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        r#"
            site "T"
            content {
              collections {
                posts "posts/**/*.typ" sort="date"
                notes "notes/**/*.typ"
              }
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
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}",
    );
    let out = sb.run(&["-c", sb.path("config.kdl").to_str().unwrap(), "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Human-facing logs live on stderr; stdout stays data-only.
    let logs = String::from_utf8_lossy(&out.stderr);
    assert!(logs.contains("built") || logs.contains("building"));
}

#[test]
fn profile_flag_applied() {
    // `serve` blocks, so it can't be run with `output()`. Spawn it, prove the
    // profile's port override took effect by connecting to it, then reap.
    let port = free_port();
    let sb = Site::new();
    sb.write(
        "config.kdl",
        &format!(
            r#"
            site "T"
            serve {{ port 1821; open #false; }}
            profiles {{
              ci {{
                serve {{ port {port}; }}
              }}
            }}
        "#
        ),
    );
    let _srv = sb.spawn(&[
        "-c",
        sb.path("config.kdl").to_str().unwrap(),
        "--profile",
        "ci",
        "serve",
        "--no-watch",
        "--open",
        "false",
    ]);
    // Default port is 1821; reachability on `port` proves the `ci` profile won.
    assert!(
        wait_for_port(port, 5000),
        "server never bound profile port {port}"
    );
}

#[test]
fn discover_with_collection_override() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        r#"
            site "T"
            paths {
              content "content"
              dist "public"
            }
            content {
              collections {
                posts permalink="/blog/{slug}/"
              }
            }
            "#,
    );
    sb.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hi\",)\nbody",
    );
    let cfg = sb.config();
    let cols = discover(&cfg, &project(&cfg)).unwrap();
    let posts = cols.iter().find(|c| c.id == "posts").unwrap();
    assert_eq!(posts.pages.len(), 1);
    assert_eq!(posts.pages[0].permalink, "/blog/hello/");
}
