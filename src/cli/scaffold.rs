use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use owo_colors::OwoColorize;

use crate::cli::output::{Paths, Report};
use crate::cli::prompt::Prompt;
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
                report.muted(format_args!("  {} {}", "+".green(), Paths(&dir.display().to_string())))?;
            }
        }
        for (rel, contents) in &self.files {
            let full = self.root.join(rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full, contents)?;
            report.muted(format_args!("  {} {}", "+".green(), Paths(&rel.display().to_string())))?;
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

    /// Resolve the template file for a collection, defaulting to `post.typ`.
    fn template_for(&self, collection: &str) -> String {
        self.collection(collection)
            .and_then(|c| c.template.clone())
            .unwrap_or_else(|| "post.typ".into())
    }
}

/// Scaffold a new project. `yes` takes the default VCS without asking; `vcs`
/// pins a specific one.
pub fn init(report: &mut Report, root: &Path, yes: bool, vcs: Option<Vcs>) -> Result<()> {
    report.milestone(format_args!(
        "initializing project in {}",
        root.display().dimmed()
    ))?;
    Scaffold::new(root)
        .dir("content")
        .dir("assets")
        .dir("templates")
        .file("config.kdl", templates::CONFIG)
        .file("content/index.typ", templates::INDEX)
        .file("content/posts/hello.typ", templates::HELLO)
        .file("templates/post.typ", templates::POST_LAYOUT)
        .apply(report)?;
    if let Some(vcs) = Repo::wanted(yes, vcs)? {
        Repo::new(root, vcs).setup(report)?;
    }
    report.success("project ready")?;
    report.muted(format_args!("run {} to build", "baudelaire build".cyan()))?;
    Ok(())
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
        match Command::new(cmd).args(args).current_dir(self.root).status() {
            Ok(status) if status.success() => report.muted(format_args!(
                "  {} {} repository",
                "+".green(),
                self.vcs.label().dimmed()
            )),
            Ok(_) => report.warn(format_args!("{} init failed; skipped repository setup", cmd)),
            Err(_) => report.warn(format_args!("{cmd} not found; skipped repository setup")),
        }?;
        Ok(())
    }
}

/// Scaffold templates, embedded from `scaffold/` at build time. Editing those
/// files — not string literals here — changes what `init`/`new` produce.
mod templates {
    pub const CONFIG: &str = include_str!("scaffold/config.kdl");
    pub const INDEX: &str = include_str!("scaffold/index.typ");
    pub const HELLO: &str = include_str!("scaffold/hello.typ");
    pub const POST_LAYOUT: &str = include_str!("scaffold/post.typ");
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
