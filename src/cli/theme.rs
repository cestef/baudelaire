//! `baudelaire theme`: what the shipped themes are, and how one gets into a project.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use owo_colors::OwoColorize;

use super::{Cx, Run};
use crate::config::Config;
use crate::error::Result;
use crate::ui::{Count, Paths};

#[derive(Args, Debug, Clone)]
pub struct ThemeArgs {
    #[command(subcommand)]
    pub what: ThemeCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ThemeCommand {
    /// List the themes this binary ships, and what the project has installed.
    #[command(visible_alias = "ls")]
    List,
    /// Copy a theme into the project, from wherever it is.
    Add(ThemeAddArgs),
    /// Report what a theme declares, and what an installed copy has become.
    Info(ThemeArgsFor),
    /// Rewrite an installed copy from this binary, keeping your edits.
    #[command(visible_alias = "up")]
    Update(ThemeUpdateArgs),
    /// Take an installed copy back off.
    #[command(visible_aliases = ["rm", "uninstall"])]
    Remove(ThemeUpdateArgs),
}

#[derive(Args, Debug, Clone)]
pub struct ThemeAddArgs {
    #[command(flatten)]
    pub theme: ThemeArgsFor,

    /// The directory *inside* the source that holds the theme, for a repository
    /// or an archive that carries a whole project. Recorded, so `update` goes
    /// back to the same place.
    #[arg(long, value_name = "PATH")]
    pub subdir: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct ThemeArgsFor {
    /// Which theme: a name `baudelaire theme list` prints, or, for `add`, where
    /// to get one (a directory, `@namespace/name:version`, `gh:owner/repo`, a
    /// repository URL with an optional `#ref`, or an archive URL).
    pub name: String,

    /// Where it lives, if not `themes/<name>`. It has to stay inside the
    /// project: a Typst import cannot reach outside the root.
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct ThemeUpdateArgs {
    #[command(flatten)]
    pub theme: ThemeArgsFor,

    /// Replace (or delete) the files you have edited too. Without it they are
    /// kept and reported, which is the point of the record `add` leaves.
    #[arg(long)]
    pub force: bool,
}

impl Run for ThemeArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let says = Says::of(cx);
        match &self.what {
            ThemeCommand::List => {
                Self::list(cx, &says);
                Ok(())
            }
            ThemeCommand::Add(args) => args.add(cx, &says),
            ThemeCommand::Info(args) => args.info(cx, &says),
            ThemeCommand::Update(args) => args.update(cx, &says),
            ThemeCommand::Remove(args) => args.remove(cx, &says),
        }
    }
}

/// What the project's config says to a theme verb: where a theme lives, and
/// where a fetch may go for one. A config that is missing or will not parse
/// says nothing, and every question falls back to its default.
struct Says {
    theme: Option<String>,
    fetching: crate::theme::Fetching,
}

impl Says {
    fn of(cx: &Cx) -> Self {
        let config = cx.cli.config().ok();
        Self {
            theme: config.as_ref().and_then(|config| config.theme.clone()),
            fetching: config.as_ref().map(Into::into).unwrap_or_default(),
        }
    }

    fn theme(&self) -> Option<&str> {
        self.theme.as_deref()
    }
}

impl ThemeArgs {
    /// The shelf, with what this project has taken off it: a theme already
    /// installed says where, and whether the copy still matches the binary.
    fn list(cx: &Cx, says: &Says) {
        use crate::theme::{BUNDLED, Bundled, Lock};

        for theme in BUNDLED {
            cx.ui.arrow(theme.name, theme.about);
            let rel = theme.dir(says.theme());
            if let Some(lock) = Lock::read(&cx.root.join(&rel)) {
                cx.ui.item(Self::installed(cx, &rel, &lock));
            }
        }

        let mut mine: Vec<(PathBuf, Lock)> = Self::vendored(cx, says)
            .into_iter()
            .filter(|(_, lock)| Bundled::find(&lock.theme).is_err())
            .collect();
        mine.sort_by(|a, b| a.1.theme.cmp(&b.1.theme));
        if !mine.is_empty() {
            cx.ui.section("yours");
            for (rel, lock) in mine {
                cx.ui.arrow(&lock.theme, lock.origin().label());
                cx.ui.item(Self::installed(cx, &rel, &lock));
            }
        }

        cx.ui.detail(format_args!(
            "{} writes one into the project",
            "baudelaire theme add <spec>".cyan()
        ));
    }

    /// Every installed copy this project has: the directories under `themes/`,
    /// plus wherever the config's own `theme` line points, which is the one
    /// place a copy can be that nothing else would look.
    fn vendored(cx: &Cx, says: &Says) -> Vec<(PathBuf, crate::theme::Lock)> {
        use crate::theme::{Bundled, Lock};

        let named = says.theme().map(PathBuf::from).into_iter();
        let under = std::fs::read_dir(cx.root.join(Bundled::DIR))
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .map(|entry| Path::new(Bundled::DIR).join(entry.file_name()));
        let mut seen = std::collections::BTreeSet::new();
        named
            .chain(under)
            .filter(|rel| seen.insert(rel.clone()))
            .filter_map(|rel| {
                let lock = Lock::read(&cx.root.join(&rel))?;
                Some((rel, lock))
            })
            .collect()
    }

    /// One copy's line: where it is, and how much of it is yours now. Styled for
    /// the terminal, never marked up: the reporting methods write an
    /// `impl Display` straight out, backticks and all.
    fn installed(cx: &Cx, rel: &Path, lock: &crate::theme::Lock) -> String {
        use crate::theme::State;

        let at = rel.display().to_string();
        let edited = lock
            .state(&cx.root.join(rel))
            .iter()
            .filter(|file| file.state == State::Edited)
            .count();
        match edited {
            0 => format!("installed at {}", Paths(&at)),
            n => format!("installed at {}, {} edited", Paths(&at), Count::files(n)),
        }
    }
}

/// A shipped theme and where this project keeps its copy, resolved once so no
/// two verbs act on different directories.
struct Vendored {
    name: String,
    /// Relative to the project, as every message spells it.
    rel: PathBuf,
    /// The same directory, absolute: what the file operations take.
    dir: PathBuf,
}

impl Vendored {
    fn at(&self) -> String {
        self.rel.display().to_string()
    }

    /// The files a run left alone; `why` is the caller's, since `update` and
    /// `remove` keep them for different reasons.
    fn kept(cx: &Cx, files: &[&crate::theme::Tracked], why: String) {
        if files.is_empty() {
            return;
        }
        cx.ui.section("kept");
        for file in files {
            cx.ui.item(Paths(&file.rel.display().to_string()));
        }
        cx.ui.detail(why);
    }
}

impl ThemeArgsFor {
    /// Where this project keeps the theme called `name`: `--dir` when the run
    /// says so, otherwise what the config's `theme` line names. `--dir` is
    /// checked here and only here, since the verbs write to and delete from the
    /// directory it addresses and a Typst import cannot reach outside the root.
    fn vendored(&self, cx: &Cx, configured: Option<&str>, name: &str) -> Result<Vendored> {
        let rel = match &self.dir {
            None => crate::theme::Bundled::directory(name, configured),
            Some(dir) => crate::fs::Contained::new(dir)
                .ok_or_else(|| crate::error::ThemeError::outside(&dir.display().to_string()))?
                .path()
                .to_path_buf(),
        };
        Ok(Vendored {
            dir: cx.root.join(&rel),
            name: name.to_owned(),
            rel,
        })
    }
}

impl ThemeAddArgs {
    fn add(&self, cx: &Cx, says: &Says) -> Result<()> {
        use crate::theme::Origin;

        let origin = Origin::parse(&self.theme.name)?.within(self.subdir.clone());
        let fetched = origin.fetch(&says.fetching)?;
        let this = self.theme.vendored(cx, says.theme(), &fetched.name)?;
        let written = fetched.install(&this.dir)?;
        let at = this.at();
        cx.ui.done(match written.len() {
            0 => format!("{} is already there", Paths(&at)),
            n => format!("wrote {} to {}", Count::files(n), Paths(&at)),
        });
        if let Some(about) = &fetched.about {
            cx.ui.detail(about);
        }
        cx.ui.section("next");
        cx.ui.arrow(Config::FILE, format!("theme \"{at}\"").cyan());
        cx.ui.item(format_args!(
            "then {}; the theme's README says what it reads from a page",
            "baudelaire build".cyan()
        ));
        Ok(())
    }
}

impl ThemeArgsFor {
    /// What the theme is, and what this project's copy of it has become. Read
    /// off the copy in the project whenever there is one, never by fetching.
    fn info(&self, cx: &Cx, says: &Says) -> Result<()> {
        use crate::theme::{Bundled, Lock, State};

        let this = self.vendored(cx, says.theme(), &self.name)?;
        let carried = Bundled::find(&self.name).ok();
        cx.ui.done_plain(this.name.cyan());
        if let Some(theme) = carried {
            cx.ui.detail(theme.about);
        }

        let Some(lock) = Lock::read(&this.dir) else {
            if let Some(theme) = carried {
                Ships::of(&theme.fetched()).print(cx);
            }
            cx.ui.section("installed");
            cx.ui.detail(format_args!(
                "not here; {} writes it",
                format!("baudelaire theme add {}", this.name).cyan()
            ));
            return Ok(());
        };

        Ships::at(&this.dir).print(cx);
        cx.ui.section("installed");
        cx.ui.arrow("at", Paths(&this.at()));
        cx.ui.arrow("from", lock.origin().label());
        cx.ui.arrow(
            "written by",
            format_args!("baudelaire {}", lock.baudelaire.cyan()),
        );
        for file in lock.state(&this.dir) {
            let state = match file.state {
                State::Pristine => continue,
                State::Edited => "edited",
                State::Gone => "deleted",
                State::Added => "new in this version",
                State::Yours => "yours, not the theme's",
            };
            cx.ui.item(format_args!(
                "{state} {}",
                Paths(&file.rel.display().to_string())
            ));
        }
        Ok(())
    }
}

/// What a theme declares, however it is being read: out of the binary, or off
/// the copy in the project.
struct Ships {
    templates: Vec<String>,
    files: usize,
    /// The theme's own `theme.kdl`, parsed as the build parses it.
    defaults: Option<Config>,
}

impl Ships {
    /// A theme in hand.
    fn of(fetched: &crate::theme::Fetched) -> Self {
        let paths: Vec<PathBuf> = fetched.paths().map(Path::to_path_buf).collect();
        Self::new(&paths, fetched.text(Path::new(crate::theme::Theme::CONFIG)))
    }

    /// A theme installed in the project, read off the disk.
    fn at(dir: &Path) -> Self {
        let files: Vec<PathBuf> = crate::theme::Lock::present(dir).into_iter().collect();
        let defaults = std::fs::read_to_string(dir.join(crate::theme::Theme::CONFIG)).ok();
        Self::new(&files, defaults)
    }

    fn new(paths: &[PathBuf], defaults: Option<String>) -> Self {
        Self {
            templates: paths
                .iter()
                .filter_map(|rel| rel.strip_prefix(crate::theme::Theme::TEMPLATES).ok())
                .map(|rel| rel.display().to_string())
                .collect(),
            files: paths.len(),
            defaults: defaults.and_then(|text| Config::parse(&text).ok()),
        }
    }

    fn print(&self, cx: &Cx) {
        cx.ui.section("ships");
        cx.ui.arrow("templates", self.templates.join(", "));
        cx.ui.arrow("files", self.files.to_string());
        let Some(config) = &self.defaults else {
            return;
        };
        let say = |label: &str, names: Vec<&str>| {
            if !names.is_empty() {
                cx.ui.arrow(label, names.join(", "));
            }
        };
        say(
            "collections",
            config
                .content
                .collections
                .iter()
                .map(|(id, _)| id.as_str())
                .collect(),
        );
        say(
            "taxonomies",
            config
                .content
                .taxonomies
                .iter()
                .map(|(id, _)| id.as_str())
                .collect(),
        );
    }
}

impl ThemeUpdateArgs {
    /// Bring a copy up to what its source has now. The copy's own record says
    /// where it came from; a copy with no record reads the name on the command
    /// line as a spec.
    fn update(&self, cx: &Cx, says: &Says) -> Result<()> {
        use crate::theme::{Lock, Origin, State};

        let this = self.theme.vendored(cx, says.theme(), &self.theme.name)?;
        let origin = match Lock::read(&this.dir) {
            Some(lock) => lock.origin(),
            None => Origin::parse(&self.theme.name)?,
        };
        let tracked = origin
            .fetch(&says.fetching)?
            .update(&this.dir, self.force)?;
        cx.ui.done(format_args!(
            "{} is at baudelaire {}",
            Paths(&this.at()),
            crate::VERSION.cyan()
        ));
        let kept: Vec<_> = if self.force {
            Vec::new()
        } else {
            tracked
                .iter()
                .filter(|file| matches!(file.state, State::Edited | State::Yours))
                .collect()
        };
        Vendored::kept(
            cx,
            &kept,
            format!(
                "yours, so they were left alone; {} replaces them too",
                "--force".cyan()
            ),
        );
        Ok(())
    }

    fn remove(&self, cx: &Cx, says: &Says) -> Result<()> {
        use crate::theme::{Lock, State};

        let this = self.theme.vendored(cx, says.theme(), &self.theme.name)?;
        let tracked = Lock::uninstall(&this.dir, self.force)?;
        cx.ui.done(format_args!(
            "removed {} from {}",
            this.name.cyan(),
            Paths(&this.at())
        ));
        let kept: Vec<_> = tracked
            .iter()
            .filter(|file| match file.state {
                State::Edited => !self.force,
                State::Yours => true,
                _ => false,
            })
            .collect();
        Vendored::kept(
            cx,
            &kept,
            format!(
                "not baudelaire's to delete; {} takes the edited ones too",
                "--force".cyan()
            ),
        );
        cx.ui.section("next");
        cx.ui.arrow(
            Config::FILE,
            format_args!(
                "drop the {} line",
                format!("theme \"{}\"", this.at()).cyan()
            ),
        );
        Ok(())
    }
}
