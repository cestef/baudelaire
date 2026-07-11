use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use owo_colors::OwoColorize;

use crate::cli::output::{Paths, Report};
use crate::cli::prompt::{Input, Prompt};
use crate::config::Config;
use crate::error::Result;
use crate::fs;

/// Declarative scaffold: dirs + files to create under a root.
struct Scaffold<'a> {
    root: &'a Path,
    dirs: Vec<PathBuf>,
    files: Vec<(PathBuf, String)>,
}

impl<'a> Scaffold<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            dirs: Vec::new(),
            files: Vec::new(),
        }
    }

    fn dir(mut self, rel: impl Into<PathBuf>) -> Self {
        self.dirs.push(rel.into());
        self
    }

    fn file(mut self, rel: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files.push((rel.into(), contents.into()));
        self
    }

    fn apply(self, report: &mut Report) -> Result<()> {
        for dir in &self.dirs {
            let path = self.root.join(dir);
            if !path.exists() {
                fs::create_dir_all(&path)?;
                report.muted(format_args!(
                    "  {} {}",
                    "+".green(),
                    Paths(&dir.display().to_string())
                ))?;
            }
        }
        for (rel, contents) in &self.files {
            let full = self.root.join(rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full, contents)?;
            report.muted(format_args!(
                "  {} {}",
                "+".green(),
                Paths(&rel.display().to_string())
            ))?;
        }
        Ok(())
    }
}

impl Config {
    /// Infer the collection id from a content path's directory components.
    fn collection_for(&self, path: &Path) -> String {
        for seg in path.components() {
            let name = seg.as_os_str().to_str().unwrap_or("");
            if self.collection(name).is_some() {
                return name.to_owned();
            }
        }
        "posts".to_owned()
    }

    /// Resolve the template file for a collection, defaulting to `layout.typ`.
    fn template_for(&self, collection: &str) -> String {
        self.collection(collection)
            .and_then(|c| c.template.clone())
            .unwrap_or_else(|| "layout.typ".into())
    }
}

/// Scaffold a new project. `yes` takes every prompt's default without asking;
/// `vcs` pins a version-control system.
pub fn init(report: &mut Report, root: &Path, yes: bool, vcs: Option<Vcs>) -> Result<()> {
    report.milestone(format_args!(
        "initializing project in {}",
        Paths(&root.display().to_string())
    ))?;

    let interactive = !yes && std::io::stdin().is_terminal();
    let details = Details::gather(root, interactive)?;
    let repo = Repo::wanted(yes, vcs)?;
    if interactive {
        report.blank()?;
    }

    Scaffold::new(root)
        .dir("content")
        .dir("assets")
        .dir("templates")
        .file("config.kdl", details.config())
        .file("content/index.typ", templates::INDEX)
        .file("content/posts/hello.typ", templates::HELLO)
        .file("templates/layout.typ", templates::LAYOUT)
        .file("assets/style.css", templates::STYLE)
        .apply(report)?;

    if let Some(vcs) = repo {
        Repo::new(root, vcs).setup(report)?;
    }

    report.blank()?;
    report.success("project ready")?;
    report.muted(format_args!(
        "run {} to build, {} for a live preview",
        "baudelaire build".cyan(),
        "baudelaire serve".cyan()
    ))?;
    Ok(())
}

/// Project metadata for a fresh scaffold: prompted interactively, or defaulted
/// from the target directory name and git config.
struct Details {
    site: String,
    author: String,
    url: String,
}

impl Details {
    fn gather(root: &Path, interactive: bool) -> Result<Self> {
        let name = Self::dir_name(root);
        let author = Self::git_author().unwrap_or_default();
        if !interactive {
            return Ok(Self {
                site: name,
                author,
                url: "https://example.com".into(),
            });
        }
        Ok(Self {
            site: Input::new("Site name").default(&name).ask()?,
            author: Input::new("Author").default(&author).ask()?,
            url: Input::new("Base URL")
                .default("https://example.com")
                .ask()?,
        })
    }

    /// The scaffolded `config.kdl`, its placeholders filled in.
    fn config(&self) -> String {
        templates::render(
            templates::CONFIG,
            &[
                ("site", &self.site),
                ("author", &self.author),
                ("url", &self.url),
            ],
        )
    }

    /// A sensible default site name from the target directory's own name.
    fn dir_name(root: &Path) -> String {
        root.file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
            })
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "my-site".into())
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

/// Scaffold a new content file, inferring collection + template from config.
pub fn new_page(report: &mut Report, path: &Path, config: &Config) -> Result<()> {
    if path.exists() {
        return Err(crate::error::ScaffoldError::already_exists(path).into());
    }
    let collection = config.collection_for(path);
    let template = config.template_for(&collection);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("untitled.typ");
    let body = templates::render(templates::PAGE, &[("template", &template)]);
    Scaffold::new(path.parent().unwrap_or(Path::new(".")))
        .file(name, body)
        .apply(report)?;
    report.success(format_args!("created {}", path.display()))?;
    Ok(())
}

/// A version-control system baudelaire can initialize for a new project. Both
/// use the same `.gitignore` (jujutsu honors it too).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Vcs {
    Git,
    #[value(alias = "jj")]
    Jujutsu,
}

impl Vcs {
    /// The command that initializes a repository, and the marker directory whose
    /// presence means one already exists. Jujutsu colocates a `.git`, so it stays
    /// interoperable with git tooling.
    fn init(self) -> (&'static [&'static str], &'static str) {
        match self {
            Self::Git => (&["git", "init", "-q"], ".git"),
            Self::Jujutsu => (&["jj", "git", "init", "--colocate"], ".jj"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Jujutsu => "jujutsu",
        }
    }
}

/// Optional version-control setup for a freshly scaffolded project: a
/// `.gitignore` plus a repository. Opt-in, because not every scaffold wants one.
struct Repo<'a> {
    root: &'a Path,
    vcs: Vcs,
}

impl<'a> Repo<'a> {
    const IGNORE: &'static str = include_str!("scaffold/gitignore");

    fn new(root: &'a Path, vcs: Vcs) -> Self {
        Self { root, vcs }
    }

    /// Which VCS to set up, if any. An explicit `--vcs` wins; `yes` takes the
    /// default (git) without asking; otherwise ask, but only on an interactive
    /// terminal — piped or CI input defaults to none, so a scaffold never blocks
    /// waiting for an answer nor creates a repo unbidden.
    fn wanted(yes: bool, explicit: Option<Vcs>) -> Result<Option<Vcs>> {
        if explicit.is_some() {
            return Ok(explicit);
        }
        if yes {
            return Ok(Some(Vcs::Git));
        }
        if !std::io::stdin().is_terminal() {
            return Ok(None);
        }
        // git is the default (empty answer); every option lists its aliases.
        Prompt::new("set up version control?")
            .option(&["git", "g", "y", "yes"], Some(Vcs::Git))
            .default()
            .option(&["jj", "jujutsu", "j"], Some(Vcs::Jujutsu))
            .option(&["no", "n"], None)
            .ask()
    }

    /// Write `.gitignore` and initialize the repository, skipping either step if
    /// it already exists. A missing or failing tool is a warning, not an error:
    /// the project is scaffolded either way.
    fn setup(&self, report: &mut Report) -> Result<()> {
        let ignore = self.root.join(".gitignore");
        if !ignore.exists() {
            fs::write(&ignore, Self::IGNORE)?;
            report.muted(format_args!("  {} {}", "+".green(), Paths(".gitignore")))?;
        }
        let (argv, marker) = self.vcs.init();
        if self.root.join(marker).exists() {
            return Ok(());
        }
        let (cmd, args) = argv.split_first().expect("non-empty argv");
        // Capture the tool's output rather than inherit it — jj in particular
        // prints an "Initialized repo" line and a hint that would clutter the
        // scaffold log. Surface it only if the command actually failed.
        match Command::new(cmd).args(args).current_dir(self.root).output() {
            Ok(out) if out.status.success() => report.muted(format_args!(
                "  {} {} repository",
                "+".green(),
                self.vcs.label().dimmed()
            ))?,
            Ok(out) => {
                report.warn(format_args!("{cmd} init failed; skipped repository setup"))?;
                let detail = String::from_utf8_lossy(&out.stderr);
                if !detail.trim().is_empty() {
                    report.item(detail.trim())?;
                }
            }
            Err(_) => report.warn(format_args!("{cmd} not found; skipped repository setup"))?,
        }
        Ok(())
    }
}

/// Scaffold templates, embedded from `scaffold/` at build time. Editing those
/// files — not string literals here — changes what `init`/`new` produce.
mod templates {
    /// Site config; `{{site}}`, `{{author}}`, `{{url}}` are filled by [`render`].
    pub const CONFIG: &str = include_str!("scaffold/config.kdl");
    pub const INDEX: &str = include_str!("scaffold/index.typ");
    pub const HELLO: &str = include_str!("scaffold/hello.typ");
    pub const LAYOUT: &str = include_str!("scaffold/layout.typ");
    pub const STYLE: &str = include_str!("scaffold/style.css");
    /// New-page template; `{{template}}` is filled by [`render`].
    pub const PAGE: &str = include_str!("scaffold/page.typ");

    /// Substitute `{{key}}` placeholders in a template.
    pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
        let mut out = template.to_owned();
        for (key, value) in vars {
            out = out.replace(&format!("{{{{{key}}}}}"), value);
        }
        out
    }
}
