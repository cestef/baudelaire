//! What an asset compiler may import: the project and its asset layers, and
//! never the rest of the machine, since a theme's scripts are fetched code.

use crate::common::Site;

/// A site with a local theme, and a secret beside the project rather than in it.
fn site() -> (Site, tempfile::TempDir) {
    let site = Site::with(
        r#"
        site "T"
        theme "themes/evil"
        paths { content "content"; dist "public"; assets "assets" }
        "#,
    );
    site.write("themes/evil/theme.kdl", "assets {\n  bundle #true\n}\n");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nHi.\n",
    );
    let outside = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        outside.path().join("secret.json"),
        r#"{"key":"SECRET-1234"}"#,
    )
    .expect("write");
    std::fs::write(
        outside.path().join("_secret.scss"),
        "$leak: \"SECRET-1234\";",
    )
    .expect("write");
    (site, outside)
}

#[cfg(feature = "js")]
#[test]
fn a_script_cannot_import_a_file_outside_the_project() {
    let (site, outside) = site();
    let secret = outside.path().join("secret.json");
    site.write(
        "themes/evil/assets/app.js",
        &format!(
            "import s from {:?};\nconsole.log(s.key);\n",
            secret.display().to_string()
        ),
    );

    let err = site.build_error().to_string();

    assert!(err.contains("outside the project"), "{err}");
    assert!(!site.exists("public/assets/app.js"));
}

#[cfg(feature = "js")]
#[test]
fn a_script_still_imports_its_neighbours_and_the_project() {
    let (site, _outside) = site();
    site.write("data/names.json", r#"{"name":"in-project"}"#);
    site.write(
        "themes/evil/assets/util.js",
        "export const n = \"neighbour\";\n",
    );
    site.write(
        "themes/evil/assets/app.js",
        "import { n } from \"./util.js\";\nimport d from \"../../../data/names.json\";\nconsole.log(n, d.name);\n",
    );

    site.stats();

    let js = site.output("assets/app.js");
    assert!(
        js.contains("neighbour") && js.contains("in-project"),
        "{js}"
    );
}

#[cfg(feature = "sass")]
#[test]
fn a_stylesheet_cannot_use_a_file_outside_the_project() {
    let (site, outside) = site();
    let secret = outside.path().join("secret");
    site.write(
        "themes/evil/assets/style.scss",
        &format!(
            "@import {:?};\nbody::after {{ content: $leak; }}\n",
            secret.display().to_string()
        ),
    );

    let err = site.build_error().to_string();

    assert!(err.contains("outside the project"), "{err}");
    assert!(!site.exists("public/assets/style.css"));
}

#[cfg(feature = "sass")]
#[test]
fn a_stylesheet_still_uses_its_partials() {
    let (site, _outside) = site();
    site.write("themes/evil/assets/_colors.scss", "$ink: #123456;\n");
    site.write(
        "themes/evil/assets/style.scss",
        "@use \"colors\";\nbody { color: colors.$ink; }\n",
    );

    site.stats();

    assert!(site.output("assets/style.css").contains("#123456"));
}
