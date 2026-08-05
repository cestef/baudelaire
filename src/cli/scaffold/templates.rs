//! The files `init` and `new` write, and the placeholder substitution over them.
//!
//! Embedded from `scaffold/` at build time. Editing those files (not string
//! literals here) changes what `init`/`new` produce.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use include_dir::{Dir, include_dir};
use itertools::Itertools as _;

/// One starter project shape.
///
/// The directory *is* the manifest: every file under it is written at the
/// same relative path, so adding a file to a template is adding a file, not
/// a file plus a table entry.
pub struct Template {
    /// What `--template` accepts, and what the summary reports.
    pub name: &'static str,
    /// One line for `--help` and for the unknown-name suggestion.
    pub about: &'static str,
    files: Dir<'static>,
}

/// The registered starter templates.
///
/// Single source of truth: the flag's default, its help listing, the
/// resolution of a given name and the "did you mean" on a typo all read this
/// one table, so a new template is one entry and cannot drift out of the
/// help text.
pub const TEMPLATES: &[Template] = &[
    Template {
        name: "blog",
        about: "dated posts, tags, pagination and feeds",
        files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/blog"),
    },
    Template {
        name: "docs",
        about: "ordered sections, sidebar nav and client-side search",
        files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/docs"),
    },
    Template {
        name: "book",
        about: "ordered chapters, also exported as one HTML file",
        files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/book"),
    },
    Template {
        name: "minimal",
        about: "one page and one template, nothing else",
        files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/minimal"),
    },
];

/// One optional feature `--with` switches on.
pub struct Extra {
    /// What `--with` accepts.
    pub name: &'static str,
    /// The KDL appended verbatim to the rendered config. A duplicate
    /// top-level block is not a conflict: [`crate::config`] dispatches every
    /// node in order and a nested section fills in place, so a second
    /// `generate { .. }` merges into the first rather than replacing it.
    pub fragment: &'static str,
    /// Whether the starter shape already asked for this. A shape that
    /// configures the feature itself (the `docs` one configures search, with
    /// fields and a palette) would otherwise be handed a second, barer block
    /// saying the same thing.
    pub present: fn(&crate::config::Config) -> bool,
}

/// The registered optional features.
///
/// Single source of truth for what `--with` accepts, exactly as
/// [`TEMPLATES`] is for `--template`.
pub const EXTRAS: &[Extra] = &[
    Extra {
        name: "spa",
        fragment: "\n// Client-side navigation between the built pages.\nnavigation {\n  spa { }\n}\n",
        present: |config| config.navigation.spa.enabled,
    },
    Extra {
        name: "standalone",
        fragment: "\n// Also emit the whole site as one self-contained HTML file.\nnavigation {\n  standalone { }\n}\n",
        present: |config| config.navigation.standalone.enabled,
    },
    Extra {
        name: "speculation",
        fragment: "\n// Browser-native prefetch hints for same-site links.\nnavigation {\n  speculation { }\n}\n",
        present: |config| config.navigation.speculation.enabled,
    },
    Extra {
        name: "search",
        fragment: "\n// Client-side search index.\ngenerate {\n  search { formats \"json\" }\n}\n",
        present: |config| !config.generate.search.formats.is_empty(),
    },
    Extra {
        name: "pdf",
        fragment: "\n// A PDF of every page, from `templates/print.typ`.\ngenerate {\n  pdf { pages { template \"print.typ\" } }\n}\n",
        present: |config| config.generate.pdf.enabled(),
    },
];

/// The shape `--theme` scaffolds: identity, paths and a preview, and no
/// opinion about anything a theme declares. Deliberately not in
/// [`TEMPLATES`]: it is not a shape to choose, it is what choosing a theme
/// leaves for the project to say. A starter shape here instead would write
/// its own `collections` over the theme's (a list replaces rather than
/// merges) and bind layouts the theme does not ship.
pub const THEMED: Template = Template {
    name: "themed",
    about: "templates, assets and collections from the theme",
    files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/themed"),
};

impl Template {
    /// The shape `init` scaffolds when `--template` is not given: the first
    /// registered one, so the flag's default cannot name a missing table row.
    pub const DEFAULT: &'static str = TEMPLATES[0].name;

    /// The shape a run scaffolds: [`THEMED`] whenever a theme was named,
    /// else the chosen starter, else the default one.
    ///
    /// Naming both is not an error, since `--template` is how the four
    /// shapes are usually reached, but the theme wins and says so: what a
    /// starter shape would have contributed is exactly what the theme
    /// already declares.
    pub fn select(
        chosen: Option<&str>,
        themed: bool,
        ui: &crate::ui::Ui,
    ) -> crate::error::Result<&'static Self> {
        if themed {
            if let Some(name) = chosen {
                // Resolved first, so a typo is still an error rather than
                // something the theme silently excuses.
                let shape = Self::find(name)?;
                ui.detail(format_args!(
                    "the theme supplies the shape; `{}` is not used",
                    shape.name
                ));
            }
            return Ok(&THEMED);
        }
        Self::find(chosen.unwrap_or(Self::DEFAULT))
    }

    /// The template `name` selects, or an error naming the valid ones.
    pub fn find(name: &str) -> crate::error::Result<&'static Self> {
        TEMPLATES.iter().find(|t| t.name == name).ok_or_else(|| {
            let names = Self::names();
            crate::error::ScaffoldError::unknown_template(
                name,
                crate::config::dispatch::Keys::of(&names).help(name, "templates"),
            )
            .into()
        })
    }

    fn names() -> Vec<&'static str> {
        TEMPLATES.iter().map(|t| t.name).collect()
    }

    /// The `--template` help line, listing the table's own rows so a new
    /// shape cannot ship undocumented.
    pub fn help() -> String {
        format!("Starter shape: {}", Self::names().iter().format(", "))
    }

    /// The template's files, placeholders already substituted. Ordered so
    /// the scaffold summary is stable.
    pub fn files(&self, vars: &Vars<'_>) -> Vec<File> {
        let mut out = Vec::new();
        Self::walk(&self.files, vars, &mut out);
        out.sort_by(|a, b| a.rel.cmp(&b.rel));
        out
    }

    fn walk(dir: &Dir<'_>, vars: &Vars<'_>, out: &mut Vec<File>) {
        for file in dir.files() {
            // Every scaffolded file is text we authored, so a non-UTF-8 one
            // is a bug here rather than something to degrade over.
            let body = file.contents_utf8().expect("scaffold files are UTF-8");
            out.push(File {
                rel: file.path().to_path_buf(),
                body: vars.render(body),
            });
        }
        for sub in dir.dirs() {
            Self::walk(sub, vars, out);
        }
    }
}

impl Extra {
    /// The extras `names` selects, or an error naming the valid ones. A typo
    /// is refused rather than dropped: an unmatched `--with` used to
    /// scaffold a site missing the very feature it asked for, silently.
    pub fn resolve(names: &[String]) -> crate::error::Result<Vec<&'static Self>> {
        names.iter().map(|name| Self::find(name)).collect()
    }

    /// The extras that still have something to add, given what the starter
    /// shape's own config already declares. A shape that configures the
    /// feature is left alone and reported, rather than handed a second block
    /// that says less than the one already there.
    ///
    /// A config that does not parse yields every extra: appending is then
    /// the honest thing to do, and the parse error is not this function's to
    /// report (a test parses every shipped shape).
    pub fn wanted(
        extras: &[&'static Self],
        files: &[File],
        ui: &crate::ui::Ui,
    ) -> Vec<&'static Self> {
        let Some(config) = files
            .iter()
            .find(|f| f.is_config())
            .and_then(|f| crate::config::Config::parse(&f.body).ok())
        else {
            return extras.to_vec();
        };
        extras
            .iter()
            .filter(|extra| {
                let present = (extra.present)(&config);
                if present {
                    ui.detail(format_args!("{} is already part of this shape", extra.name));
                }
                !present
            })
            .copied()
            .collect()
    }

    fn find(name: &str) -> crate::error::Result<&'static Self> {
        EXTRAS.iter().find(|e| e.name == name).ok_or_else(|| {
            let names = Self::names();
            crate::error::ScaffoldError::unknown_extra(
                name,
                crate::config::dispatch::Keys::of(&names).help(name, "features"),
            )
            .into()
        })
    }

    fn names() -> Vec<&'static str> {
        EXTRAS.iter().map(|e| e.name).collect()
    }

    /// The `--with` help line, listing the table's own rows.
    pub fn help() -> String {
        format!(
            "Switch on optional features: {}",
            Self::names().iter().format(", ")
        )
    }
}

/// One file a starter shape ships: where it lands under the project root,
/// and its rendered contents.
pub struct File {
    pub rel: PathBuf,
    pub body: String,
}

impl File {
    /// The fixed layout every starter shape shares. The flags that skip or
    /// rewrite a scaffolded file all decide from these, not from the
    /// configured `paths { }`: these are the paths the shipped `config.kdl`
    /// declares.
    const CONFIG: &'static str = "config.kdl";
    const HOME: &'static str = "content/index.typ";
    const CONTENT: &'static str = "content";

    /// Whether this is the config `init` bolts its flags onto.
    pub fn is_config(&self) -> bool {
        self.rel == Path::new(Self::CONFIG)
    }

    /// Where the scaffolded config lands: whatever the global `--config`
    /// names, so a project initialized under one name is one every later
    /// command finds under the same flag. The flag used to be accepted and
    /// ignored, writing `config.kdl` and reporting success.
    ///
    /// Only a bare filename can serve: a `paths { }` entry resolves against
    /// the working directory rather than against the config file, so a
    /// config nested a directory down would name a content tree outside its
    /// own project.
    pub fn config_at(path: &Path) -> crate::error::Result<PathBuf> {
        if path.file_name().is_none_or(|name| name != path.as_os_str()) {
            return Err(crate::error::ScaffoldError::config_path(path).into());
        }
        Ok(path.to_path_buf())
    }

    /// Whether this is a demo page `--no-sample` drops. The home page is not
    /// one: a site with no content at all does not build to anything.
    pub fn sample(&self) -> bool {
        self.rel.starts_with(Self::CONTENT) && self.rel != Path::new(Self::HOME)
    }
}

/// The placeholder values a template is rendered against.
pub struct Vars<'a>(Vec<(&'a str, &'a str)>);

impl<'a> Vars<'a> {
    pub fn new(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self(pairs.into_iter().collect())
    }

    /// Substitute `{{key}}` placeholders in a template, in a single left-to-
    /// right pass: a substituted value is never rescanned, so a site name
    /// containing `{{author}}` stays literal. Values are escaped for the
    /// double-quoted string context they land in, so a quote in a site name
    /// yields valid config. Unknown placeholders are left untouched.
    pub fn render(&self, template: &str) -> String {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(open) = rest.find("{{") {
            out.push_str(&rest[..open]);
            let after = &rest[open + 2..];
            let Some(close) = after.find("}}") else {
                // No closing braces anywhere ahead: emit the rest verbatim.
                out.push_str(&rest[open..]);
                return out;
            };
            let key = &after[..close];
            if let Some((_, value)) = self.0.iter().find(|(k, _)| *k == key) {
                let _ = write!(out, "{}", Quoted(value));
            } else {
                out.push_str("{{");
                out.push_str(key);
                out.push_str("}}");
            }
            rest = &after[close + 2..];
        }
        out.push_str(rest);
        out
    }
}

/// A value escaped for the double-quoted string literal it is interpolated
/// into. KDL and typst share `\` and `"` escapes, so one adapter serves both
/// the rendered templates and the config fragments `init` appends.
pub struct Quoted<'a>(pub &'a str);

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            if matches!(c, '"' | '\\') {
                f.write_char('\\')?;
            }
            f.write_char(c)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{EXTRAS, Extra, File, TEMPLATES, THEMED, Template, Vars};

    fn vars() -> Vars<'static> {
        Vars::new([
            ("site", "My Site"),
            ("author", "Me"),
            ("url", "https://example.com"),
            ("lang", "en"),
        ])
    }

    /// A starter config is what a new site begins from, so every one has to
    /// stay valid against the dispatch tables. Nothing else reads them,
    /// which is exactly how a key rename ships a broken `init`.
    #[test]
    fn every_scaffolded_config_parses() {
        for template in TEMPLATES {
            let files = template.files(&vars());
            let text = &files
                .iter()
                .find(|f| f.is_config())
                .unwrap_or_else(|| panic!("`{}` has no config.kdl", template.name))
                .body;
            let config = crate::config::Config::parse(text)
                .unwrap_or_else(|e| panic!("`{}` config: {e}", template.name));
            // every declared profile has to apply, not merely parse
            for (name, _) in config.profiles.clone() {
                config
                    .clone()
                    .with_profile(&name)
                    .unwrap_or_else(|e| panic!("`{}` profile `{name}`: {e}", template.name));
            }
        }
    }

    /// `--config` names the file `init` writes, and only a bare name can:
    /// a nested one would leave every `paths { }` entry pointing outside
    /// the project it was scaffolded into.
    #[test]
    fn the_scaffolded_config_takes_a_name_not_a_path() {
        use std::path::Path;
        assert_eq!(
            File::config_at(Path::new("site.kdl")).unwrap(),
            Path::new("site.kdl")
        );
        assert!(File::config_at(Path::new("conf/site.kdl")).is_err());
        assert!(File::config_at(Path::new("/etc/site.kdl")).is_err());
        assert!(File::config_at(Path::new("../site.kdl")).is_err());
    }

    /// Every template ships a home page and a template to render it with,
    /// the two files without which the scaffold does not build.
    #[test]
    fn every_template_is_complete() {
        for template in TEMPLATES {
            let files = template.files(&vars());
            assert!(
                files.iter().any(File::is_config),
                "`{}` has no config",
                template.name
            );
            assert!(
                files
                    .iter()
                    .any(|f| f.rel == std::path::Path::new(File::HOME)),
                "`{}` has no home page",
                template.name
            );
            assert!(
                files
                    .iter()
                    .any(|f| f.rel.starts_with("templates") && f.rel.extension().is_some()),
                "`{}` ships no template",
                template.name
            );
            assert!(
                !files.iter().any(|f| f.body.contains("{{")),
                "`{}` left a placeholder unfilled",
                template.name
            );
        }
    }

    /// A `--with` fragment is appended to a config that may already carry
    /// the same section, so each has to survive the merge on its own.
    #[test]
    fn every_extra_parses_onto_every_template() {
        for template in TEMPLATES {
            let files = template.files(&vars());
            let base = &files.iter().find(|f| f.is_config()).expect("config").body;
            for extra in EXTRAS {
                let text = format!("{base}{}", extra.fragment);
                crate::config::Config::parse(&text)
                    .unwrap_or_else(|e| panic!("`{}` + --with {}: {e}", template.name, extra.name));
            }
        }
    }

    /// The theme shape states nothing a theme declares. A `collections` list
    /// of its own would replace the theme's whole set rather than merge
    /// with it, and a `template` key would name a file the theme never
    /// ships: between them, that was a scaffold whose first build failed.
    #[test]
    fn the_theme_shape_leaves_the_theme_its_own_declarations() {
        let files = THEMED.files(&vars());
        let config = crate::config::Config::parse(
            &files.iter().find(|f| f.is_config()).expect("config").body,
        )
        .expect("the theme shape's config parses");
        assert!(config.content.collections.is_empty());
        assert!(config.content.taxonomies.is_empty());
        assert!(
            !files.iter().any(|f| f.rel.starts_with("templates")),
            "the theme ships the templates"
        );
        for file in &files {
            assert!(
                !file.body.contains("template:"),
                "`{}` binds a template the theme may not have",
                file.rel.display()
            );
        }
    }

    /// Naming a theme picks the theme shape whatever `--template` says,
    /// since a starter shape's config is exactly what the theme declares.
    /// A misspelled shape is still an error: the theme does not excuse it.
    #[test]
    fn a_theme_selects_the_theme_shape_over_any_starter() {
        let ui = crate::ui::Ui::new(crate::ui::Level::Silent);
        assert_eq!(Template::select(None, true, &ui).unwrap().name, "themed");
        assert_eq!(
            Template::select(Some("docs"), true, &ui).unwrap().name,
            "themed"
        );
        assert_eq!(
            Template::select(None, false, &ui).unwrap().name,
            Template::DEFAULT
        );
        assert_eq!(
            Template::select(Some("book"), false, &ui).unwrap().name,
            "book"
        );
        assert!(Template::select(Some("blogg"), true, &ui).is_err());
    }

    /// A feature the shape already configures is dropped rather than
    /// appended: the `docs` shape sets `search` with its fields and its
    /// palette, and a second, barer block underneath said less.
    #[test]
    fn an_extra_the_shape_already_configures_is_dropped() {
        let ui = crate::ui::Ui::new(crate::ui::Level::Silent);
        let docs = Template::find("docs").expect("docs shape");
        let files = docs.files(&vars());
        let asked = Extra::resolve(&["search".to_owned(), "pdf".to_owned()]).expect("features");
        let wanted = Extra::wanted(&asked, &files, &ui);
        assert_eq!(
            wanted.iter().map(|e| e.name).collect::<Vec<_>>(),
            vec!["pdf"]
        );
    }

    /// An unknown template names the real ones rather than failing bare.
    #[test]
    fn an_unknown_template_suggests_a_real_one() {
        let Err(err) = Template::find("blogg") else {
            panic!("`blogg` is not a template");
        };
        let rendered = format!("{err:?}");
        assert!(rendered.contains("blogg"), "{rendered}");
        assert!(
            rendered.contains("blog"),
            "suggests the real one: {rendered}"
        );
    }

    /// An unknown `--with` feature is refused the same way, rather than
    /// scaffolding a site quietly missing what it asked for.
    #[test]
    fn an_unknown_extra_suggests_a_real_one() {
        let Err(err) = Extra::resolve(&["serach".to_owned()]) else {
            panic!("`serach` is not a feature");
        };
        let rendered = format!("{err:?}");
        assert!(rendered.contains("serach"), "{rendered}");
        assert!(
            rendered.contains("search"),
            "suggests the real one: {rendered}"
        );
    }

    #[test]
    fn fills_known_placeholders() {
        let out =
            Vars::new([("site", "S"), ("author", "A")]).render("site \"{{site}}\" by {{author}}");
        assert_eq!(out, "site \"S\" by A");
    }

    #[test]
    fn escapes_quotes_and_backslashes_in_values() {
        let out = Vars::new([("site", "My \"Quoted\\\" Site")]).render("site \"{{site}}\"");
        assert_eq!(out, "site \"My \\\"Quoted\\\\\\\" Site\"");
    }

    #[test]
    fn substituted_values_are_never_rescanned() {
        let out =
            Vars::new([("site", "{{author}}"), ("author", "Me")]).render("{{site}} by {{author}}");
        assert_eq!(out, "{{author}} by Me");
    }

    #[test]
    fn unknown_placeholders_are_left_alone() {
        assert_eq!(
            Vars::new([("site", "S")]).render("keep {{unknown}}"),
            "keep {{unknown}}"
        );
    }

    #[test]
    fn unterminated_braces_pass_through() {
        assert_eq!(
            Vars::new([("site", "S")]).render("dangling {{site"),
            "dangling {{site"
        );
    }
}
