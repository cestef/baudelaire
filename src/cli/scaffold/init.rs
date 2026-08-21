//! Scaffolding a whole project: what to write, where, and what the answers were.

use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;

use super::vcs::Repo;
use super::{Scaffold, templates};
use crate::cli::prompt::{Input, Prompt};
use crate::cli::scaffold::templates::{Extra, Quoted, Template, Vars};
use crate::cli::{InitArgs, Root};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::MirrorSkipped;
use crate::mirror::{Mirror, Settings};
use crate::ui::{Paths, Ui};

/// Scaffolding a whole project: what `init` decides, in what order, and what it
/// writes.
pub(in crate::cli) struct Init;

impl Init {
    /// Scaffold a new project: pick the starter shape, fill its placeholders from
    /// the flags (prompting for what they left out, when there is a terminal to
    /// prompt at), write the files the flags did not exclude, and optionally
    /// initialize a repository.
    pub(in crate::cli) fn run(ui: &Ui, root: &Root, args: &InitArgs, config: &Path) -> Result<()> {
        let extras = Extra::resolve(&args.with)?;
        let config = templates::File::config_at(config)?;
        let interactive = !args.yes && std::io::stdin().is_terminal();
        let start = Start::gather(args, interactive)?;
        let template = Template::select(start.template.as_deref(), start.theme.is_some(), ui)?;
        let (target, details) = Details::gather(args, root, interactive)?;
        let repo = Repo::wanted(interactive, args.vcs)?;
        if interactive {
            ui.blank();
        }

        let files = template.files(&details.vars());
        let extras = Extra::wanted(&extras, &files, ui);

        let mut scaffold = Scaffold::new(&target).ignore();
        for file in files {
            if args.no_sample && file.sample() {
                continue;
            }
            let (rel, body) = if file.is_config() {
                (
                    config.clone(),
                    Details::config(&file.body, start.theme.as_deref(), &extras),
                )
            } else {
                (file.rel, file.body)
            };
            scaffold = scaffold.file(rel, body);
        }
        scaffold.apply(ui)?;

        if let Some(vcs) = repo {
            Repo::new(&target, vcs).setup(ui);
        }

        let settings = Self::packages(ui, &target);

        ui.blank();
        ui.done_plain(format_args!(
            "{} project ready in {}",
            template.name,
            Paths(&target.display().to_string())
        ));
        ui.detail(format_args!("{}", template.about));
        ui.detail(format_args!(
            "run {} to build, {} for a live preview",
            "baudelaire build".cyan(),
            "baudelaire serve".cyan()
        ));
        if let Some(spec) = &start.theme {
            Placement::of(spec, &target).settle(ui, &target)?;
        }
        if let Some(settings) = settings {
            settings.render(ui);
        }
        Ok(())
    }

    /// Mirror the generated modules for editor tooling, so the imports the
    /// scaffolded templates carry resolve from the first minute. A failure
    /// warns rather than errors: a build never reads what this writes.
    fn packages(ui: &Ui, target: &Path) -> Option<Settings> {
        let config = Config {
            root: target.to_path_buf(),
            ..Config::default()
        };
        match Mirror::new(&config, None, false).install() {
            Ok(install) => Some(install.render(ui)),
            Err(error) => {
                ui.warn(MirrorSkipped {
                    reason: error.to_string(),
                });
                None
            }
        }
    }
}

/// What a run scaffolds from, once the flags and the prompts have both had
/// their say: the `--template` name and the `--theme` spec, either of which may
/// still be absent.
pub(super) struct Start {
    template: Option<String>,
    theme: Option<String>,
}

/// One answer to that question, carrying a table row out of the prompt.
#[derive(Clone, Copy)]
pub(super) enum Chosen {
    /// A starter shape, scaffolded in full.
    Shape(&'static Template),
    /// A theme this binary carries, which brings the shape with it.
    #[cfg(feature = "themes")]
    Themed(&'static crate::theme::Bundled),
}

impl Start {
    /// The shape and theme for this run: naming either flag settles the
    /// question, so only a run that named neither, with a terminal to ask at,
    /// is asked.
    fn gather(args: &InitArgs, interactive: bool) -> Result<Self> {
        if !interactive || args.template.is_some() || args.theme.is_some() {
            return Ok(Self {
                template: args.template.clone(),
                theme: args.theme.clone(),
            });
        }
        Ok(Self::from(Self::ask()?))
    }

    /// Ask, offering [`Chosen::all`] and letting each row describe itself.
    fn ask() -> Result<Chosen> {
        let mut prompt = Prompt::new("Start from");
        for choice in Chosen::all() {
            prompt = prompt.one(choice.name(), choice).about(choice.about());
            if choice.preselected() {
                prompt = prompt.default();
            }
        }
        prompt.ask()
    }
}

impl Chosen {
    /// Every answer the question offers: the starter shapes, then the themes
    /// this binary carries.
    fn all() -> Vec<Self> {
        let shapes = templates::TEMPLATES.iter().map(Self::Shape);
        #[cfg(feature = "themes")]
        let shapes = shapes.chain(crate::theme::BUNDLED.iter().map(Self::Themed));
        shapes.collect()
    }

    /// The word that picks it, which is also its label.
    fn name(self) -> &'static str {
        match self {
            Self::Shape(template) => template.name,
            #[cfg(feature = "themes")]
            Self::Themed(theme) => theme.name,
        }
    }

    /// The one line it describes itself with.
    fn about(self) -> &'static str {
        match self {
            Self::Shape(template) => template.about,
            #[cfg(feature = "themes")]
            Self::Themed(theme) => theme.about,
        }
    }

    /// Whether this is the answer an unanswered prompt takes, which is also the
    /// shape a non-interactive run scaffolds.
    fn preselected(self) -> bool {
        match self {
            Self::Shape(template) => template.name == Template::DEFAULT,
            #[cfg(feature = "themes")]
            Self::Themed(_) => false,
        }
    }
}

impl From<Chosen> for Start {
    /// A chosen theme becomes the directory spec `--theme` documents
    /// (`themes/<name>`), which [`Placement`] then writes the theme into.
    fn from(chosen: Chosen) -> Self {
        match chosen {
            Chosen::Shape(template) => Self {
                template: Some(template.name.to_owned()),
                theme: None,
            },
            #[cfg(feature = "themes")]
            Chosen::Themed(theme) => Self {
                template: None,
                theme: Some(format!("{}/{}", crate::theme::Bundled::DIR, theme.name)),
            },
        }
    }
}

/// What a `--theme` spec asks of the scaffold: a directory theme is a path
/// inside the project, and `init` runs before anyone has put one there.
pub(super) enum Placement<'a> {
    /// A package spec, or a directory already in place: the build resolves it.
    Resolved,
    /// A directory spec whose name is one of the shipped themes, which this run
    /// writes there.
    #[cfg(feature = "themes")]
    Shipped(&'a str, &'static crate::theme::Bundled),
    /// A directory spec naming a theme baudelaire does not carry: the reader
    /// has to put it there.
    Missing(&'a str),
}
impl<'a> Placement<'a> {
    /// Read the spec against what the freshly scaffolded project holds. A
    /// package spec (`@local/name:1.0.0`) is resolved from a package directory
    /// rather than the project, so it is nothing this can check.
    fn of(spec: &'a str, target: &Path) -> Self {
        if spec.starts_with('@') || target.join(spec).is_dir() {
            return Self::Resolved;
        }
        #[cfg(feature = "themes")]
        if let Some(theme) = crate::theme::Bundled::named_by(spec) {
            return Self::Shipped(spec, theme);
        }
        Self::Missing(spec)
    }

    /// Write what can be written, and say what cannot; the signature is the
    /// same in both feature flavors, so the caller is too.
    #[cfg_attr(
        not(feature = "themes"),
        allow(unused_variables, clippy::unnecessary_wraps)
    )]
    fn settle(&self, ui: &Ui, target: &Path) -> Result<()> {
        match self {
            Self::Resolved => {}
            #[cfg(feature = "themes")]
            Self::Shipped(spec, theme) => {
                let written = theme.fetched().install(&target.join(spec))?;
                ui.section("theme");
                ui.arrow(
                    theme.name,
                    format_args!("{} files to {}", written.len(), Paths(spec)),
                );
                ui.item(theme.about.dimmed());
            }
            Self::Missing(spec) => {
                ui.section("theme");
                ui.arrow("missing", Paths(spec));
                ui.item(
                    format_args!(
                        "put its directory there before building; {}/start/themes/ covers what a theme is",
                        env!("CARGO_PKG_HOMEPAGE")
                    )
                    .dimmed(),
                );
            }
        }
        Ok(())
    }
}

/// Project metadata for a fresh scaffold: prompted interactively, or defaulted
/// from the target directory name and git config.
pub(super) struct Details {
    site: String,
    author: String,
    url: String,
    lang: String,
}

impl Details {
    /// The site name a run falls back to when nothing names one.
    const UNNAMED: &'static str = "my-site";

    /// The author a run falls back to when nothing names one; visibly a
    /// placeholder, so it gets edited rather than shipped.
    const UNSIGNED: &'static str = "Your Name";

    /// Where to scaffold, and what to fill the placeholders with. A flag always
    /// wins; what a flag did not supply is prompted for when there is a
    /// terminal to prompt at, and otherwise defaulted.
    fn gather(args: &InitArgs, root: &Root, interactive: bool) -> Result<(PathBuf, Self)> {
        let git = crate::git::Repo::author(root.path())
            .unwrap_or_else(|| Self::UNSIGNED.to_owned());
        let ask = |label: &str, default: &str, given: Option<&String>| -> Result<String> {
            match given {
                Some(v) => Ok(v.clone()),
                None if interactive => Input::new(label).default(default).ask(),
                None => Ok(default.to_owned()),
            }
        };

        let (target, site) = match &args.dir {
            Some(d) => (d.clone(), Self::dir_name(d, root)),
            None if interactive && args.title.is_none() => {
                let site = Input::new("Site name").default(Self::UNNAMED).ask()?;
                (PathBuf::from(&site), site)
            }
            None => (PathBuf::from("."), Self::dir_name(Path::new("."), root)),
        };
        let site = args.title.as_ref().map_or(site, std::clone::Clone::clone);
        let author = ask("Author", &git, args.author.as_ref())?;
        let url = ask("Base URL", "https://example.com", args.url.as_ref())?;
        if !crate::config::BaseUrl::absolute(&url) {
            return Err(crate::error::ScaffoldError::RelativeUrl { url }.into());
        }

        Ok((
            target,
            Self {
                site,
                author,
                url,
                lang: args.lang.clone(),
            },
        ))
    }

    /// The placeholder values every scaffolded file is rendered against.
    fn vars(&self) -> Vars<'_> {
        Vars::new([
            ("site", self.site.as_str()),
            ("author", self.author.as_str()),
            ("url", self.url.as_str()),
            ("lang", self.lang.as_str()),
        ])
    }

    /// The scaffolded `config.kdl`: the template's own, plus whatever the flags
    /// bolt on. Both additions are appended rather than spliced, which a KDL
    /// config tolerates because a repeated section fills in place.
    fn config(rendered: &str, theme: Option<&str>, extras: &[&'static Extra]) -> String {
        let mut out = rendered.to_owned();
        if let Some(theme) = theme {
            let _ = write!(
                out,
                "\n// Templates, assets and config defaults come from this package.\ntheme \"{}\"\n",
                Quoted(theme)
            );
        }
        for extra in extras {
            out.push_str(extra.fragment);
        }
        out
    }

    /// A sensible default site name from the target directory's last component.
    fn dir_name(target: &Path, root: &Root) -> String {
        root.join(target)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .map_or_else(|| Self::UNNAMED.to_owned(), str::to_owned)
    }

}

#[cfg(test)]
mod start_tests {
    use super::{Chosen, Start, templates::Template};
    use crate::cli::{Cli, Command, InitArgs};

    /// The `init` flags a command line carries, parsed as the CLI parses them.
    fn args(flags: &[&str]) -> InitArgs {
        use clap::Parser as _;
        let cli = Cli::parse_from(["baudelaire", "init"].iter().chain(flags));
        let Some(Command::Init(args)) = cli.command else {
            panic!("expected init");
        };
        args
    }

    #[test]
    fn a_named_shape_or_theme_is_never_asked_about() {
        let named = Start::gather(&args(&["--template", "docs"]), true).unwrap();
        assert_eq!(named.template.as_deref(), Some("docs"));
        assert_eq!(named.theme, None);

        let themed = Start::gather(&args(&["--theme", "themes/mine"]), true).unwrap();
        assert_eq!(themed.template, None);
        assert_eq!(themed.theme.as_deref(), Some("themes/mine"));

        let both = Start::gather(&args(&["--template", "docs", "--theme", "t/x"]), true).unwrap();
        assert_eq!(both.template.as_deref(), Some("docs"));
        assert_eq!(both.theme.as_deref(), Some("t/x"));
    }

    #[test]
    fn a_run_with_no_terminal_names_neither() {
        let start = Start::gather(&args(&[]), false).unwrap();
        assert_eq!(start.template, None);
        assert_eq!(start.theme, None);
    }

    #[test]
    fn every_shape_and_every_theme_is_offered() {
        let offered: Vec<&str> = Chosen::all().iter().map(|c| c.name()).collect();
        let expected = super::templates::TEMPLATES.iter().map(|t| t.name);
        #[cfg(feature = "themes")]
        let expected = expected.chain(crate::theme::BUNDLED.iter().map(|t| t.name));
        assert_eq!(offered, expected.collect::<Vec<_>>());
        assert!(
            Chosen::all().iter().all(|c| !c.about().is_empty()),
            "every row describes itself"
        );
    }

    #[test]
    fn exactly_one_answer_is_preselected_and_it_is_the_default_shape() {
        let preselected: Vec<&str> = Chosen::all()
            .iter()
            .filter(|c| c.preselected())
            .map(|c| c.name())
            .collect();
        assert_eq!(preselected, vec![Template::DEFAULT]);
    }

    #[test]
    fn choosing_a_shape_names_it() {
        let start = Start::from(Chosen::Shape(Template::find("book").unwrap()));
        assert_eq!(start.template.as_deref(), Some("book"));
        assert_eq!(start.theme, None);
    }

    #[cfg(feature = "themes")]
    #[test]
    fn choosing_a_theme_installs_it_where_the_config_names_it() {
        let target = tempfile::tempdir().expect("tempdir");
        for theme in crate::theme::BUNDLED {
            let start = Start::from(Chosen::Themed(theme));
            assert_eq!(start.template, None);
            let spec = start.theme.expect("a theme spec");
            assert_eq!(spec, format!("themes/{}", theme.name));
            assert!(
                matches!(
                    super::Placement::of(&spec, target.path()),
                    super::Placement::Shipped(..)
                ),
                "`{spec}` has to be one this binary writes"
            );
        }
    }
}
