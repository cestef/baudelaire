mod common;

use baudelaire::content::Discovery;

use common::{Site, free_port, project, wait_for_port};

#[test]
fn root_flag_builds_from_that_directory() {
    let sb = Site::new();
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
    assert!(!sb.exists("public"));
}

#[test]
fn help_groups_global_flags_and_shows_examples() {
    let sb = Site::new();
    let out = sb.run(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    for heading in ["Project:", "Logging:", "Examples:"] {
        assert!(
            help.contains(heading),
            "missing `{heading}` in help:\n{help}"
        );
    }
    assert!(
        help.contains("baudelaire serve --open"),
        "help shows an example invocation"
    );
    assert!(!help.contains('\x1b'), "no escape codes when not a TTY");

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
    // `--yes` because a full sweep off a terminal refuses rather than
    // answering for itself.
    let out = sb.run(&[
        "-c",
        sb.path("config.kdl").to_str().unwrap(),
        "clean",
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!cfg.paths.dist.exists(), "dist still exists");
    assert!(!cfg.cache.dir.exists(), "cache still exists");
}

/// The wholesale sweep takes announce state with it, which is what the next
/// `announce` reconciles a live repository against.
#[test]
fn clean_refuses_a_full_sweep_it_cannot_confirm() {
    let sb = Site::new();
    sb.write("config.kdl", "site \"T\"\npaths {\n  dist \"public\"\n}\n");
    sb.write("public/index.html", "<html></html>");
    let cfg = sb.config();
    let out = sb.run(&["-c", sb.path("config.kdl").to_str().unwrap(), "clean"]);
    assert!(!out.status.success(), "expected a refusal");
    assert!(cfg.paths.dist.exists(), "dist was removed anyway");

    let out = sb.run(&[
        "-c",
        sb.path("config.kdl").to_str().unwrap(),
        "clean",
        "--output",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!cfg.paths.dist.exists(), "dist still exists");
}

/// `clean` is the recovery command, so it must not depend on the artifact most
/// likely to be broken.
#[test]
fn clean_falls_back_to_the_defaults_when_the_config_does_not_parse() {
    let sb = Site::new();
    sb.write("config.kdl", "site \"T\"\nthis is not { valid kdl\n");
    sb.write("public/index.html", "<html></html>");
    let out = sb.run(&[
        "-c",
        sb.path("config.kdl").to_str().unwrap(),
        "clean",
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!sb.path("public").exists(), "default dist still exists");
}

/// A config that is missing entirely still fails: sweeping `public` out of
/// whatever directory you were standing in is not a recovery.
#[test]
fn clean_still_refuses_a_missing_config() {
    let sb = Site::new();
    sb.write("public/index.html", "<html></html>");
    let out = sb.run(&[
        "-c",
        sb.path("config.kdl").to_str().unwrap(),
        "clean",
        "--yes",
    ]);
    assert!(!out.status.success(), "expected a failure");
    assert!(sb.path("public").exists(), "swept without a project");
}

/// `new` writes the file even when the project cannot be opened. The two things
/// it loses are conveniences: the next `order` and the collision check.
#[test]
fn new_scaffolds_without_a_readable_project() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\ntypst {\n  features \"no-such-feature\"\n}\n",
    );
    let out = sb.run(&[
        "-c",
        sb.path("config.kdl").to_str().unwrap(),
        "new",
        "posts/hello",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sb.path("content/posts/hello.typ").exists(), "no page");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("scaffolding without it"),
        "stderr: {stderr}"
    );
}

/// `--version` answers "what am I holding": the version, where it was built
/// from, and which optional capabilities are compiled in.
#[test]
fn version_reports_the_build_and_its_features() {
    let sb = Site::new();
    let out = sb.run(&["--version"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("baudelaire "), "{stdout}");
    for label in ["commit", "rustc", "target", "flavor", "features"] {
        assert!(stdout.contains(label), "no `{label}` row: {stdout}");
    }
    assert!(
        !stdout.contains('\u{1b}'),
        "escapes survived a pipe: {stdout}"
    );

    let out = sb.run(&["-V"]);
    assert!(out.status.success());
    let short = String::from_utf8_lossy(&out.stdout);
    assert_eq!(short.lines().count(), 1, "{short}");
    assert!(short.starts_with("baudelaire "), "{short}");
}

/// `--dry-run` names every directory and removes none.
#[test]
fn clean_dry_run_reports_without_removing() {
    let sb = Site::new();
    sb.write("config.kdl", "site \"T\"\npaths {\n  dist \"public\"\n}\n");
    sb.write("public/index.html", "<html></html>");
    let cfg = sb.config();
    let out = sb.run(&[
        "-c",
        sb.path("config.kdl").to_str().unwrap(),
        "clean",
        "--dry-run",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("public"), "stderr: {stderr}");
    assert!(cfg.paths.dist.exists(), "dist was removed by a dry run");
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
                posts "posts/**/*.typ" { sort "date" }
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
    let logs = String::from_utf8_lossy(&out.stderr);
    assert!(logs.contains("built") || logs.contains("building"));
}

#[test]
fn profile_flag_applied() {
    // `serve` blocks, so it cannot be run with `output()`.
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
        "--no-open",
    ]);
    // The default port is 1821, so reaching `port` proves the profile won.
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
                posts { permalink "/blog/{slug}/" }
              }
            }
            "#,
    );
    sb.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hi\",)\nbody",
    );
    let cfg = sb.config();
    let cols = Discovery::all(&cfg, &project(&cfg)).unwrap();
    let posts = cols.iter().find(|c| c.id == "posts").unwrap();
    assert_eq!(posts.pages.len(), 1);
    assert_eq!(posts.pages[0].permalink, "/blog/hello/");
}

/// `--strict` turns the warning tally into an exit code, which CI can otherwise
/// only find by grepping stderr.
#[test]
fn strict_fails_a_run_that_warned() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    // A broken internal link, demoted to a warning by `--no-strict-links`: a
    // run that warns and would otherwise succeed.
    sb.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\n#link(\"/content/gone.typ\")[gone]\n",
    );
    let config = sb.path("config.kdl");
    let config = config.to_str().unwrap();

    let lax = sb.run(&["-c", config, "build", "--no-strict-links"]);
    let stderr = String::from_utf8_lossy(&lax.stderr).into_owned();
    assert!(
        stderr.contains("warning"),
        "the fixture should warn, else this proves nothing: {stderr}"
    );
    assert!(
        lax.status.success(),
        "a warning alone must not fail: {stderr}"
    );

    let strict = sb.run(&["-c", config, "--strict", "build", "--no-strict-links"]);
    assert!(!strict.status.success(), "--strict should have failed");
    let stderr = String::from_utf8_lossy(&strict.stderr);
    assert!(
        stderr.contains("baudelaire::strict::warnings"),
        "expected the typed diagnostic: {stderr}"
    );
}

/// ...and a clean run is unaffected, so `--strict` is safe to leave on in CI.
#[test]
fn strict_passes_a_run_that_did_not_warn() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    sb.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome\n",
    );
    let config = sb.path("config.kdl");
    let out = sb.run(&["-c", config.to_str().unwrap(), "--strict", "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A publishing command builds the site it is about to send, so `--json` has to
/// count those pages.
#[test]
fn json_counts_the_pages_a_publishing_command_built() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\nurl \"https://x.test\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n\
         deploy {\n  ssh {\n    host \"h\"\n    path \"/tmp/nowhere\"\n  }\n}\n",
    );
    sb.write("content/p.typ", "#let frontmatter = (title: \"P\",)\nx");
    let config = sb.path("config.kdl");
    let out = sb.run(&[
        "-c",
        config.to_str().unwrap(),
        "--json",
        "deploy",
        "--dry-run",
    ]);

    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout should be one JSON object");
    assert_eq!(report["pages"], 1, "{report}");
    assert_eq!(report["cached"], 0, "{report}");
}

/// A warning reaches the report through `Ui::warn`; the error that stopped the
/// build passes through neither, so it has to be added on its own.
#[test]
fn json_reports_the_error_that_failed_the_run() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"nope\"\n  dist \"public\"\n}\n",
    );
    let config = sb.path("config.kdl");
    let out = sb.run(&["-c", config.to_str().unwrap(), "--json", "build"]);
    assert!(!out.status.success(), "the build should have failed");

    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout should be one JSON object");
    assert_eq!(report["ok"], false);
    let fatal = report["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|d| d["severity"] == "error")
        .unwrap_or_else(|| panic!("no error diagnostic in {report}"));
    assert_eq!(fatal["code"], "baudelaire::fs::read_directory");
    assert!(
        fatal["message"].as_str().is_some_and(|m| !m.is_empty()),
        "{report}"
    );
}

/// `--json` is the only thing that ever writes to stdout, so a CI job can read
/// the run as data instead of scraping styled prose off stderr.
#[test]
fn json_writes_a_machine_readable_summary_to_stdout() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\ncheck { links \"warn\" }\n",
    );
    sb.write("content/a.typ", "#let frontmatter = (title: \"A\",)\na");
    sb.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\n#link(\"/content/gone.typ\")[bad]\n",
    );
    let config = sb.path("config.kdl");
    let out = sb.run(&["-c", config.to_str().unwrap(), "--json", "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout should be one JSON object");
    // Asserted against the constant, so bumping it stays one deliberate edit.
    assert_eq!(report["schema"], baudelaire::ui::Report::SCHEMA);
    assert_eq!(report["ok"], true);
    assert_eq!(report["pages"], 2);
    assert_eq!(report["warnings"], 1);
    // Found by code rather than by position, since a build may also report
    // advice this test is not about.
    let broken = report["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|d| d["code"] == "baudelaire::links::broken")
        .unwrap_or_else(|| panic!("no broken-link diagnostic in {report}"));
    assert_eq!(broken["severity"], "warning");
    assert!(
        broken["message"].as_str().is_some_and(|m| !m.is_empty()),
        "{broken}"
    );
}

/// Every shell the value list offers produces a script, and it goes to stdout
/// clean: a banner in front of it would be sourced along with the completions.
///
/// Driven off clap's own value list, so a shell added to `Shell` is covered.
#[test]
fn completions_are_generated_for_every_offered_shell() {
    use clap::ValueEnum;

    let sb = Site::new();
    for shell in baudelaire::cli::Shell::value_variants() {
        let name = shell.to_possible_value().expect("shell is selectable");
        let name = name.get_name();
        let out = sb.run(&["completions", name]);
        assert!(
            out.status.success(),
            "{name}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let script = String::from_utf8(out.stdout).expect("script is utf-8");
        assert!(
            script.contains("baudelaire"),
            "{name}: script never names the binary: {script}"
        );
        assert!(script.contains("serve"), "{name}: no subcommands: {script}");
    }
}

/// An unknown shell is a usage error naming the ones that exist, not a stack of
/// help text, and not an empty file the user would happily source.
#[test]
fn an_unknown_shell_is_a_usage_error() {
    let sb = Site::new();
    let out = sb.run(&["completions", "tcsh"]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty(), "stdout: {:?}", out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("tcsh"), "{stderr}");
    assert!(stderr.contains("bash"), "{stderr}");
}

/// Every short alias reaches the command it names, and none of them collide.
///
/// The pairs are written out rather than derived, so changing one is an edit
/// here too: an alias is an API the moment it ships.
#[test]
fn short_aliases_reach_their_commands() {
    let sb = Site::new();
    let pairs = [
        ("b", "build"),
        ("s", "serve"),
        ("c", "check"),
        ("n", "new"),
        ("d", "deploy"),
        ("cl", "clean"),
        ("i", "init"),
        ("comp", "completions"),
        ("ref", "reference"),
    ];

    for (alias, full) in pairs {
        let short = sb.run(&[alias, "--help"]);
        let long = sb.run(&[full, "--help"]);
        assert!(
            short.status.success(),
            "{alias}: {}",
            String::from_utf8_lossy(&short.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&short.stdout),
            String::from_utf8_lossy(&long.stdout),
            "`{alias}` and `{full}` should be the same command"
        );
    }
}

/// The man page is roff on stdout, headed by the `.TH` line `man` needs to
/// index it. Nothing here renders it; that it parses is groff's business.
#[test]
fn man_writes_a_roff_page_to_stdout() {
    let sb = Site::new();
    let out = sb.run(&["man"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let page = String::from_utf8(out.stdout).expect("page is utf-8");
    assert!(page.contains(".TH baudelaire 1"), "{page}");
    assert!(page.contains(".SH SYNOPSIS"), "{page}");
    assert!(page.contains(env!("CARGO_PKG_VERSION")), "{page}");
}

/// ...and `--json` never appends its object to a command whose own document is
/// the stdout payload.
///
/// `--json` is global, so a wrapper that sets it once reaches these three too.
#[test]
fn json_leaves_a_document_commands_stdout_alone() {
    let sb = Site::new();
    for argv in [
        vec!["--json", "completions", "bash"],
        vec!["--json", "man"],
        vec!["--json", "reference"],
    ] {
        let out = sb.run(&argv);
        assert!(
            out.status.success(),
            "{argv:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8(out.stdout).expect("the document is utf-8");
        assert!(!stdout.is_empty(), "{argv:?} wrote no document");
        // The report is one compact object, so its opening bytes are exact.
        assert!(
            !stdout.contains("{\"schema\":"),
            "{argv:?}: a run report landed in the document"
        );
        assert!(
            !stdout.lines().last().unwrap_or_default().starts_with('{'),
            "{argv:?}: the document ends in an object"
        );
    }

    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    sb.write("content/index.typ", "#let frontmatter = (title: \"H\",)\nh");
    let out = sb.run(&["--json", "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("build still reports on stdout");
    assert_eq!(report["ok"], true);
}

/// Without the flag, stdout stays empty: that reservation is what makes the
/// object above safe to pipe.
#[test]
fn stdout_is_empty_without_json() {
    let sb = Site::new();
    sb.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    sb.write("content/index.typ", "#let frontmatter = (title: \"H\",)\nh");
    let config = sb.path("config.kdl");
    let out = sb.run(&["-c", config.to_str().unwrap(), "build"]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "stdout: {:?}", out.stdout);
}

/// `config check` is the only way to find out whether a config parses without
/// building the site it configures.
#[test]
fn config_check_validates_the_projects_own_config() {
    let sb = Site::with("site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n");
    let out = sb.run(&["config", "check"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("config.kdl"));
}

#[test]
fn config_check_fails_on_a_key_no_build_would_accept() {
    let sb = Site::with("site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n");
    sb.write(
        "wrong.kdl",
        "site \"T\"\nartifacts {\n  cards {\n    widht 100\n  }\n}\n",
    );
    let out = sb.run(&["config", "check", "--isolated", "wrong.kdl"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown config key"), "{stderr}");
    assert!(stderr.contains("did you mean `width`?"), "{stderr}");
}

/// What a documentation snippet is: config text with no project around it, and
/// no theme directory for a `theme` line to resolve against.
#[test]
fn config_check_isolated_reads_the_text_alone() {
    let sb = Site::with("site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n");
    sb.write("fragment.kdl", "theme \"themes/nowhere\"\n");

    let out = sb.run(&["config", "check", "--isolated", "fragment.kdl"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = sb.run(&["config", "check", "fragment.kdl"]);
    assert!(!out.status.success(), "a build would resolve that theme");
}
