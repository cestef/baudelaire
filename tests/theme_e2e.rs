//! Themes: templates, assets, static files, and config defaults a site inherits
//! and overrides file by file.

mod common;

use baudelaire::config::Config;
use common::Site;

/// A site with a theme in `themes/plume`, carrying a template, a stylesheet, a
/// static file, and config defaults.
fn site() -> Site {
    let site = Site::with(
        r#"
        site "T"
        theme "themes/plume"
        paths { content "content"; dist "public"; assets "assets"; static "static" }
        "#,
    );
    site.write(
        "themes/plume/theme.kdl",
        "site \"Theme default\"\nlang \"fr\"\nhtml {\n  pretty #false\n}\n",
    );
    site.write(
        "themes/plume/templates/page.typ",
        "#let page(data, body) = html.elem(\"main\")[#body]\n",
    );
    site.write("themes/plume/assets/theme.css", "body { color: red }\n");
    site.write("themes/plume/assets/shared.css", "body { margin: 0 }\n");
    site.write("themes/plume/static/robots.txt", "theme\n");
    site
}

#[test]
fn a_page_uses_the_themes_template() {
    let site = site();
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHello.\n",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("<main>"), "theme template applied: {html}");
    assert!(html.contains("Hello."), "{html}");
}

/// A template the project has shadows the theme's, without renaming anything.
#[test]
fn a_project_template_overrides_the_themes() {
    let site = site();
    site.write(
        "templates/page.typ",
        "#let page(data, body) = html.elem(\"article\")[#body]\n",
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHello.\n",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("<article>"), "project template wins: {html}");
    assert!(!html.contains("<main>"), "{html}");
}

#[test]
fn assets_and_static_files_layer_with_the_project_on_top() {
    let site = site();
    site.write("assets/shared.css", "body { margin: 8px }\n");
    site.write("assets/own.css", "body { padding: 0 }\n");
    site.write("static/humans.txt", "project\n");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nHello.\n",
    );
    site.stats();

    assert!(site.output("assets/theme.css").contains("red"));
    assert!(site.output("robots.txt").contains("theme"));
    assert!(site.output("assets/shared.css").contains("8px"));
    assert!(site.output("assets/own.css").contains("padding"));
    assert!(site.output("humans.txt").contains("project"));
}

/// `theme.kdl` is a floor, not a ceiling: keys the site states win, keys it
/// leaves out fall back to the theme's.
#[test]
fn theme_config_supplies_defaults_the_site_overrides() {
    let site = site();
    let config = Config::load(&site.read("config.kdl"), &site.root, None).expect("config");

    assert_eq!(config.site.as_deref(), Some("T"));
    assert_eq!(config.lang, "fr");
    assert!(!config.html.pretty, "nested theme default inherited");
}

/// `--theme` replaces the theme early enough that the `theme.kdl` supplying the
/// defaults is the overriding theme's.
#[test]
fn the_theme_override_supplies_its_own_defaults() {
    let site = site();
    site.write(
        "themes/other/theme.kdl",
        "lang \"de\"\nhtml {\n  pretty #true\n}\n",
    );

    let config =
        Config::load(&site.read("config.kdl"), &site.root, Some("themes/other")).expect("config");

    assert_eq!(config.theme.as_deref(), Some("themes/other"));
    assert_eq!(config.lang, "de");
    assert!(config.html.pretty);
    assert_eq!(config.site.as_deref(), Some("T"));
}

/// The same when the config names no theme at all.
#[test]
fn the_theme_override_applies_to_a_config_naming_none() {
    let site = Site::with("site \"T\"\npaths { content \"content\"; dist \"public\" }\n");
    site.write("themes/plume/theme.kdl", "lang \"fr\"\n");

    let config =
        Config::load(&site.read("config.kdl"), &site.root, Some("themes/plume")).expect("config");

    assert_eq!(config.theme.as_deref(), Some("themes/plume"));
    assert_eq!(config.lang, "fr");
}

/// ...and a floor only for what the site *builds*: a theme is fetched, so the
/// sections deciding what the machine does, or what a browser trusts in the
/// site's name, stay the site's.
#[test]
fn a_theme_cannot_set_the_sections_a_site_owns() {
    use miette::Diagnostic;

    for section in [
        "hooks {\n  before \"touch pwned\"\n}\n",
        "deploy {\n  s3 {\n    bucket \"theirs\"\n  }\n}\n",
        "announce {\n  standard {\n    did \"did:plc:x\"\n  }\n}\n",
        "paths {\n  content \"/etc\"\n}\n",
        "profiles {\n  dev {\n    hooks {\n      before \"touch pwned\"\n    }\n  }\n}\n",
        "serve {\n  editor \"sh\" \"-c\" \"touch pwned\"\n}\n",
        "typst {\n  registry \"https://theirs.test\"\n}\n",
        "security {\n  csp {\n    report \"https://theirs.test/collect\"\n  }\n}\n",
        "lint {\n  snippets {\n    sh run=\"touch pwned\"\n  }\n}\n",
    ] {
        let site = site();
        site.write("themes/plume/theme.kdl", section);

        let err = Config::load(&site.read("config.kdl"), &site.root, None).expect_err(section);

        assert_eq!(
            err.code().map(|code| code.to_string()).as_deref(),
            Some("baudelaire::theme::governs"),
            "{section}"
        );
    }
}

/// Two keys a theme may not carry from inside a section it is otherwise
/// allowed: an arbitrary response header on an arbitrary path, and a wildcard
/// `redirect`, which claims no output file for the collision check to catch.
#[test]
fn a_theme_cannot_speak_to_the_browser_in_the_sites_name() {
    use miette::Diagnostic;

    for section in [
        "generate {\n  headers {\n    \"/*\" {\n      Refresh \"0; url=https://evil.example\"\n    }\n  }\n}\n",
        "redirect {\n  \"/*\" \"https://evil.example/:splat\"\n}\n",
    ] {
        let site = site();
        site.write("themes/plume/theme.kdl", section);

        let err = Config::load(&site.read("config.kdl"), &site.root, None).expect_err(section);

        assert_eq!(
            err.code().map(|code| code.to_string()).as_deref(),
            Some("baudelaire::theme::governs"),
            "{section}"
        );
    }
}

/// ...and what stays allowed: a theme may turn the rule files on, since what
/// goes in them is computed from the site's own `caching` and `csp`.
#[test]
fn a_theme_may_still_ask_for_the_rule_files() {
    let site = site();
    site.write(
        "themes/plume/theme.kdl",
        "generate {\n  headers #true\n  redirects #true\n}\nredirect {\n  \"/old/\" \"/new/\"\n}\n",
    );

    let config = Config::load(&site.read("config.kdl"), &site.root, None).expect("config");

    assert!(config.generate.headers.enabled);
    assert!(config.generate.redirects);
    let [(old, rule)] = config.redirect.as_slice() else {
        panic!("one redirect, got {:?}", config.redirect);
    };
    assert_eq!(old, "/old/");
    assert_eq!(rule.target, "/new/");
    assert_eq!(rule.status, 301);
}

/// A theme naming a directory that is not there fails at load, naming the
/// value that is wrong.
#[test]
fn a_missing_theme_is_a_precise_error() {
    let site = Site::with("site \"T\"\ntheme \"themes/absent\"\n");
    let err = Config::load(&site.read("config.kdl"), &site.root, None).expect_err("missing theme");
    assert!(format!("{err}").contains("themes/absent"), "{err}");
}

/// The shipped themes are in the binary, so adopting one needs no download.
#[test]
#[cfg(feature = "themes")]
fn a_shipped_theme_is_written_into_the_project_and_builds() {
    let site = Site::with(
        r#"
        site "T"
        url "https://example.com"
        theme "themes/albatros"
        paths { content "content"; dist "public"; assets "assets" }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nhello",
    );
    let out = site.run(&["theme", "add", "albatros"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(site.root.join("themes/albatros/theme.kdl").is_file());

    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let html = site.read("public/index.html");
    assert!(html.contains("<header"), "no theme chrome: {html}");
}

/// The record `theme add` leaves is what tells your edits from ours.
#[test]
#[cfg(feature = "themes")]
fn an_update_keeps_what_you_changed_and_replaces_what_you_did_not() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    assert!(site.run(&["theme", "add", "albatros"]).status.success());
    assert!(site.exists("themes/albatros/.baudelaire-lock.json"));

    let page = site.path("themes/albatros/templates/page.typ");
    let shipped = std::fs::read_to_string(&page).expect("read");

    let style = site.path("themes/albatros/assets/style.css");
    std::fs::write(&style, "/* mine */\n").expect("edit");
    let home = site.path("themes/albatros/templates/home.typ");
    std::fs::remove_file(&home).expect("delete");

    // Twice, so a file kept because it was edited is still the author's on the
    // run after.
    for run in 1..=2 {
        let out = site.run(&["theme", "update", "albatros"]);
        assert!(
            out.status.success(),
            "run {run} stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(&page).expect("read"),
            shipped,
            "run {run}: an untouched file is still the binary's"
        );
        assert_eq!(
            std::fs::read_to_string(&style).expect("read"),
            "/* mine */\n",
            "run {run}: an edited file is the author's"
        );
        assert!(!home.exists(), "run {run}: a deleted file stays deleted");
    }

    assert!(
        site.run(&["theme", "update", "albatros", "--force"])
            .status
            .success()
    );
    assert_ne!(
        std::fs::read_to_string(&style).expect("read"),
        "/* mine */\n"
    );
}

/// `--dir` is the one path here a project did not resolve itself, so one that
/// climbs out of it is refused rather than written.
#[test]
#[cfg(feature = "themes")]
fn a_theme_directory_outside_the_project_is_refused() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    let out = site.run(&["theme", "add", "albatros", "--dir", "../elsewhere"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("baudelaire::theme::outside"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !site.path("../elsewhere").exists(),
        "nothing was written outside the project"
    );
}

/// Removing keeps work, the record included, so `--force` can still finish.
#[test]
#[cfg(feature = "themes")]
fn removing_refuses_to_delete_what_you_edited() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    assert!(site.run(&["theme", "add", "spleen"]).status.success());
    let style = site.path("themes/spleen/assets/style.css");
    std::fs::write(&style, "/* mine */\n").expect("edit");

    let out = site.run(&["theme", "remove", "spleen"]);
    assert!(out.status.success());
    assert!(style.exists(), "an edited file is not deleted");
    assert!(
        site.exists("themes/spleen/.baudelaire-lock.json"),
        "the record stays while it still tracks something"
    );
    assert!(
        !site.exists("themes/spleen/templates/page.typ"),
        "everything still ours is gone"
    );

    assert!(
        site.run(&["theme", "remove", "spleen", "--force"])
            .status
            .success()
    );
    assert!(!style.exists());
    assert!(
        !site.path("themes/spleen").exists(),
        "the directory goes too"
    );
}

/// A theme installed as a Typst package layers exactly as a directory one does.
///
/// Unix only, and through the binary: the package store is found under the
/// user's data directory, so this moves `HOME` for the child process.
#[test]
#[cfg(unix)]
fn a_package_theme_supplies_its_layouts_assets_and_defaults() {
    let site = Site::with(
        r#"
        site "T"
        url "https://example.net"
        theme "@local/plume:0.1.0"
        paths { content "content"; dist "public" }
        "#,
    );
    let home = site.path("home");
    let package = home.join(".local/share/typst/packages/local/plume/0.1.0");
    let write = |rel: &str, contents: &str| {
        let path = package.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, contents).expect("write");
    };
    write(
        "typst.toml",
        "[package]\nname = \"plume\"\nversion = \"0.1.0\"\nentrypoint = \"lib.typ\"\n",
    );
    write("lib.typ", "#let marker = \"from the package\"\n");
    // A relative import, the way a theme names its own pieces.
    write(
        "parts.typ",
        "#let shell(body) = html.elem(\"main\")[#body]\n",
    );
    write(
        "templates/page.typ",
        "#import \"../parts.typ\": shell\n#let page(data, body) = shell(body)\n",
    );
    write("assets/style.css", "body { color: red }\n");
    write("static/robots.txt", "package\n");
    write("theme.kdl", "lang \"fr\"\n");

    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\",)\nHello.\n",
    );

    let out = site.run_with(
        &["build"],
        // Both, because a data directory is `XDG_DATA_HOME` when it is set and
        // under `HOME` when it is not.
        &[
            ("HOME", &home.display().to_string()),
            (
                "XDG_DATA_HOME",
                &home.join(".local/share").display().to_string(),
            ),
        ],
    );
    assert!(
        out.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let html = site.output("index.html");
    assert!(html.contains("<main>"), "the package's layout ran: {html}");
    assert!(html.contains("Hello."), "{html}");
    assert!(
        html.contains("lang=\"fr\""),
        "its theme.kdl applied: {html}"
    );
    assert!(site.exists("public/robots.txt"), "its static files publish");
    assert!(
        !site.files("public/assets").is_empty(),
        "its assets publish"
    );
}

/// A theme from a directory on disk: copied in, recorded, and brought forward
/// from the same directory on the next update.
#[test]
#[cfg(feature = "themes")]
fn a_theme_is_installed_from_a_directory_and_updated_from_it() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    site.write("elsewhere/plume/templates/page.typ", "#let page = 1\n");
    site.write("elsewhere/plume/theme.kdl", "lang \"fr\"\n");

    let out = site.run(&["theme", "add", "./elsewhere/plume"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(site.read("themes/plume/theme.kdl"), "lang \"fr\"\n");
    assert!(site.exists("themes/plume/.baudelaire-lock.json"));

    // The record says where it came from, so `update` needs no second telling.
    site.write("elsewhere/plume/templates/page.typ", "#let page = 2\n");
    assert!(site.run(&["theme", "update", "plume"]).status.success());
    assert_eq!(
        site.read("themes/plume/templates/page.typ"),
        "#let page = 2\n"
    );
}

/// A theme from an archive, one directory down in a project that holds more
/// than one.
///
/// Served in-process, since none of the download, unwrapping, narrowing or
/// record cares which host the URL names.
#[test]
#[cfg(all(feature = "themes", unix))]
fn a_theme_is_fetched_out_of_an_archive_at_a_url() {
    use std::process::Command;

    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    // The shape a forge packs: everything inside one `<name>-<ref>/` directory.
    site.write(
        "packed/plume-1.0.0/themes/plume/templates/page.typ",
        "#let page = 1\n",
    );
    site.write("packed/plume-1.0.0/themes/plume/theme.kdl", "lang \"fr\"\n");
    site.write(
        "packed/plume-1.0.0/README.md",
        "a project with a theme in it\n",
    );
    let tarball = site.path("plume-1.0.0.tar.gz");
    let tarred = Command::new("tar")
        .args(["-czf", &tarball.display().to_string(), "plume-1.0.0"])
        .current_dir(site.path("packed"))
        .status()
        .expect("run tar");
    assert!(tarred.success());

    let served = std::fs::read(&tarball).expect("read");
    let host = tiny_http::Server::http("127.0.0.1:0").expect("serve");
    let port = host.server_addr().to_ip().expect("ip").port();
    let serving = std::thread::spawn(move || {
        if let Ok(request) = host.recv() {
            let _ = request.respond(tiny_http::Response::from_data(served));
        }
    });

    let url = format!("http://127.0.0.1:{port}/plume-1.0.0.tar.gz");
    let out = site.run(&["theme", "add", &url, "--subdir", "themes/plume"]);
    let _ = serving.join();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(site.read("themes/plume/theme.kdl"), "lang \"fr\"\n");
    assert!(!site.exists("themes/plume/README.md"));
    assert!(!site.exists("themes/plume/themes"));

    let lock = site.read("themes/plume/.baudelaire-lock.json");
    assert!(lock.contains("\"source\": \"archive\""), "{lock}");
    assert!(lock.contains("\"subdir\": \"themes/plume\""), "{lock}");
    assert!(lock.contains(&url), "{lock}");
}

/// A theme fetched from elsewhere is one this project has, and `list` says so.
#[test]
#[cfg(feature = "themes")]
fn list_reports_a_theme_the_binary_does_not_carry() {
    let site = Site::with("site \"T\"\nurl \"https://example.com\"\n");
    site.write("elsewhere/plume/templates/page.typ", "#let page = 1\n");
    assert!(
        site.run(&["theme", "add", "./elsewhere/plume"])
            .status
            .success()
    );

    let out = site.run(&["theme", "list"]);
    let listed = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{listed}");
    assert!(listed.contains("plume"), "the copy is listed: {listed}");
    assert!(
        listed.contains("themes/plume"),
        "with where it is: {listed}"
    );
    assert!(
        listed.contains("elsewhere/plume"),
        "and where it came from: {listed}"
    );
}
