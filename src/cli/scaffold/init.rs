//! Scaffolding a whole project: what to write, where, and what the answers were.

use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Scaffold a new project: pick the starter shape, fill its placeholders from
/// the flags (prompting for what they left out, when there is a terminal to
/// prompt at), write the files the flags did not exclude, and optionally
/// initialize a repository.
pub(crate) fn init(ui: &Ui, root: &Root, args: &InitArgs, config: &Path) -> Result<()> {
    // Every selection is resolved before anything is prompted for or written,
    // so a mistyped name fails on the spot rather than half a scaffold in.
    // These two first, because they are settled by flags alone: a mistyped
    // `--with` has to fail before the shape question, not after answering it.
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
    // What the shape already configures is not appended a second time: `--with
    // search` on a shape whose config already sets `formats`, `fields` and a
    // palette used to bolt a barer `search { formats "json" }` on beneath it.
    let extras = Extra::wanted(&extras, &files, ui);

    let mut scaffold = Scaffold::new(&target).ignore();
    for file in files {
        if args.no_sample && file.sample() {
            continue;
        }
        let (rel, body) = match file.is_config() {
            true => (
                config.clone(),
                Details::config(&file.body, start.theme.as_deref(), &extras),
            ),
            false => (file.rel, file.body),
        };
        scaffold = scaffold.file(rel, body);
    }
    scaffold.apply(ui)?;

    if let Some(vcs) = repo {
        Repo::new(&target, vcs).setup(ui);
    }

    let settings = packages(ui, &target);

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
    // Before the editor settings, because a build cannot succeed until the
    // theme is where the config says it is, and `init` naming a directory
    // nobody has put anything in yet is the ordinary case.
    if let Some(spec) = &start.theme {
        Placement::of(spec, &target).settle(ui, &target)?;
    }
    // Last, because it is the one thing here that a reader has to act on: a
    // scaffold that mirrors the modules and never says they need pointing at
    // leaves every import in the templates it just wrote marked unresolved.
    if let Some(settings) = settings {
        settings.render(ui);
    }
    Ok(())
}

/// What a run scaffolds from, once the flags and the prompts have both had
/// their say: the `--template` name and the `--theme` spec, either of which may
/// still be absent.
///
/// The two are one question, not two, and the interactive form asks it once.
/// That is not a shortcut: [`Template::select`] already lets a theme win over a
/// starter shape, because what a shape would contribute to the config is
/// exactly what a theme declares for itself.
pub(super) struct Start {
    template: Option<String>,
    theme: Option<String>,
}

/// One answer to that question. Ephemeral: it exists to carry a table row out
/// of the prompt, and [`Start`] is what the rest of `init` reads.
#[derive(Clone, Copy)]
pub(super) enum Chosen {
    /// A starter shape, scaffolded in full.
    Shape(&'static Template),
    /// A theme this binary carries, which brings the shape with it.
    #[cfg(feature = "themes")]
    Themed(&'static crate::theme::Bundled),
}

impl Start {
    /// The shape and theme for this run.
    ///
    /// A flag always wins, as everywhere else in `init`, and naming either one
    /// settles the question: a run that said `--template docs` has chosen its
    /// shape, and one that said `--theme` has chosen a theme's. Only a run that
    /// named neither, with a terminal to ask at, is asked.
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
    ///
    /// Both tables are read where they live, so a new shape or a new theme is
    /// offered by existing rather than by a second list here.
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

    /// The one line it describes itself with: the same one `--help` lists the
    /// shapes by and `theme list` prints.
    fn about(self) -> &'static str {
        match self {
            Self::Shape(template) => template.about,
            #[cfg(feature = "themes")]
            Self::Themed(theme) => theme.about,
        }
    }

    /// Whether this is the answer an unanswered prompt takes. The shape a
    /// non-interactive run scaffolds, so pressing enter and passing `--yes`
    /// produce the same project.
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
    /// (`themes/<name>`), which is what makes the answer a project that builds:
    /// the config names that directory and [`Placement`] writes the theme into
    /// it.
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
///
/// Three answers, and only the last leaves the reader with work: the shipped
/// themes are in the binary, so a spec naming one is written on the spot rather
/// than described.
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

    /// Write what can be written, and say what cannot. Installing here rather
    /// than leaving an instruction is what makes the documented one-liner
    /// (`init --theme "themes/albatros"`) a project that builds.
    ///
    /// The signature is the same in both flavors, so the caller is: with no
    /// themes carried there is nothing to write, and the `target` to write it
    /// into and the failure of writing it both go with them.
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

/// Mirror the generated modules for editor tooling, so the imports the
/// scaffolded templates and scripts carry resolve from the first minute.
///
/// A warning rather than an error: this is tooling convenience, and a platform
/// with no data directory (or one that is read-only) is no reason to fail a
/// scaffold that otherwise succeeded. The site builds either way, since a build
/// serves these modules from memory and never reads what this writes.
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

/// Project metadata for a fresh scaffold: prompted interactively, or defaulted
/// from the target directory name and git config.
pub(super) struct Details {
    site: String,
    author: String,
    url: String,
    lang: String,
}

impl Details {
    /// The site name a run falls back to when nothing names one: the prompt's
    /// default answer and the name for a directory that has none (`/`, or a
    /// path ending in `..`).
    const UNNAMED: &'static str = "my-site";

    /// The author a run falls back to when nothing names one: no `--author`,
    /// and no `user.name` in git config.
    ///
    /// A placeholder, for the same reason `url` has always had one. The fallback
    /// was the empty string, which a non-interactive `init --yes` took as its
    /// answer and wrote as `author ""`, and from there into every page's
    /// `<meta name="author">` and every feed entry: a value that is wrong
    /// everywhere and looks like nothing anywhere. `Your Name` is visibly a
    /// placeholder, so it gets edited.
    const UNSIGNED: &'static str = "Your Name";

    /// Where to scaffold, and what to fill the placeholders with.
    ///
    /// A flag always wins. What a flag did not supply is prompted for when
    /// there is a terminal to prompt at, and otherwise defaulted, so `--yes` in
    /// CI is fully scriptable rather than silently accepting `example.com`.
    ///
    /// The target directory has three cases: an explicit `dir` names it, an
    /// interactive run without one takes the site name for it, and a
    /// non-interactive run without one scaffolds into `.`.
    fn gather(args: &InitArgs, root: &Root, interactive: bool) -> Result<(PathBuf, Self)> {
        let git = Self::git_author().unwrap_or_else(|| Self::UNSIGNED.to_owned());
        // Only prompt for what was not given, and only when someone is there.
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
            // No directory, but a title (or no terminal): scaffold into `.`, the
            // shape `baudelaire init --yes` has always had in CI.
            None => (PathBuf::from("."), Self::dir_name(Path::new("."), root)),
        };
        let site = match &args.title {
            Some(t) => t.clone(),
            None => site,
        };
        let author = ask("Author", &git, args.author.as_ref())?;
        let url = ask("Base URL", "https://example.com", args.url.as_ref())?;

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

    /// The user's name from git config, if configured.
    fn git_author() -> Option<String> {
        let output = Command::new("git")
            .args(["config", "user.name"])
            .output()
            .ok()?;
        let name = String::from_utf8(output.stdout).ok()?.trim().to_owned();
        (!name.is_empty()).then_some(name)
    }
}

#[cfg(test)]
mod start_tests {
    use super::{Chosen, Start, templates::Template};
    use crate::cli::{Cli, Command, InitArgs};

    /// The `init` flags a command line carries, parsed as the CLI parses them,
    /// so a test cannot construct a combination clap would refuse.
    fn args(flags: &[&str]) -> InitArgs {
        use clap::Parser as _;
        let cli = Cli::parse_from(["baudelaire", "init"].iter().chain(flags));
        let Some(Command::Init(args)) = cli.command else {
            panic!("expected init");
        };
        args
    }

    /// Naming either flag settles the question, so nothing is asked: a run that
    /// said `--template docs` has chosen its shape, and `--theme` has chosen a
    /// theme's. `interactive` is true here, which is what makes the assertion
    /// mean something.
    #[test]
    fn a_named_shape_or_theme_is_never_asked_about() {
        let named = Start::gather(&args(&["--template", "docs"]), true).unwrap();
        assert_eq!(named.template.as_deref(), Some("docs"));
        assert_eq!(named.theme, None);

        let themed = Start::gather(&args(&["--theme", "themes/mine"]), true).unwrap();
        assert_eq!(themed.template, None);
        assert_eq!(themed.theme.as_deref(), Some("themes/mine"));

        // Both is not an error: `Template::select` resolves the shape (so a
        // typo still fails) and then lets the theme win.
        let both = Start::gather(&args(&["--template", "docs", "--theme", "t/x"]), true).unwrap();
        assert_eq!(both.template.as_deref(), Some("docs"));
        assert_eq!(both.theme.as_deref(), Some("t/x"));
    }

    /// With nobody to ask, a run names neither and falls to the default shape,
    /// which is the behaviour `--yes` and CI have always had.
    #[test]
    fn a_run_with_no_terminal_names_neither() {
        let start = Start::gather(&args(&[]), false).unwrap();
        assert_eq!(start.template, None);
        assert_eq!(start.theme, None);
    }

    /// The question offers both tables in full. A shape or a theme that shipped
    /// without being offered here would be one nobody choosing interactively
    /// could reach.
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

    /// Pressing enter and passing `--yes` scaffold the same project.
    #[test]
    fn exactly_one_answer_is_preselected_and_it_is_the_default_shape() {
        let preselected: Vec<&str> = Chosen::all()
            .iter()
            .filter(|c| c.preselected())
            .map(|c| c.name())
            .collect();
        assert_eq!(preselected, vec![Template::DEFAULT]);
    }

    /// A chosen shape is named, and nothing else is.
    #[test]
    fn choosing_a_shape_names_it() {
        let start = Start::from(Chosen::Shape(Template::find("book").unwrap()));
        assert_eq!(start.template.as_deref(), Some("book"));
        assert_eq!(start.theme, None);
    }

    /// A chosen theme becomes a spec the scaffold can act on: the config names
    /// the directory, and the theme is written into it. A spec `Placement` read
    /// as `Missing` would be an `init` that told the reader to go and find the
    /// theme it had just offered them.
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
