use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use owo_colors::OwoColorize;

use crate::cli::Root;
use crate::cli::prompt::{Input, Prompt};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::{ScaffoldExists, VcsFailed, VcsMissing};
use crate::fs;
use crate::ui::{Paths, Ui};

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

    fn apply(self, ui: &Ui) -> Result<()> {
        for dir in &self.dirs {
            let path = self.root.join(dir);
            if !path.exists() {
                fs::create_dir_all(&path)?;
                ui.detail(format_args!("{} {}", "+".green(), Paths(&dir.display().to_string())));
            }
        }
        for (rel, contents) in &self.files {
            let full = self.root.join(rel);
            // never clobber: `init` into an existing project must not overwrite
            // its config or templates. existing files are skipped with a warning.
            if full.exists() {
                ui.warn(ScaffoldExists { path: rel.clone() });
                continue;
            }
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full, contents)?;
            ui.detail(format_args!("{} {}", "+".green(), Paths(&rel.display().to_string())));
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

/// Scaffold a new project into `target` (resolved against the project `root`
/// for its default site name). `yes` takes every prompt's default without
/// asking; `vcs` pins a version-control system.
pub(crate) fn init(
    ui: &Ui,
    target: &Path,
    root: &Root,
    yes: bool,
    vcs: Option<Vcs>,
) -> Result<()> {
    let interactive = !yes && std::io::stdin().is_terminal();
    let details = Details::gather(target, root, interactive)?;
    let repo = Repo::wanted(yes, interactive, vcs)?;
    if interactive {
        ui.blank();
    }

    Scaffold::new(target)
        .dir("content")
        .dir("assets")
        .dir("templates")
        .file("config.kdl", details.config())
        .file("content/index.typ", templates::INDEX)
        .file("content/posts/hello.typ", templates::HELLO)
        .file("templates/layout.typ", templates::LAYOUT)
        .file("assets/style.css", templates::STYLE)
        .apply(ui)?;

    if let Some(vcs) = repo {
        Repo::new(target, vcs).setup(ui)?;
    }

    ui.blank();
    ui.done(format_args!("project ready in {}", Paths(&target.display().to_string())));
    ui.detail(format_args!(
        "run {} to build, {} for a live preview",
        "baudelaire build".cyan(),
        "baudelaire serve".cyan()
    ));
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
    fn gather(target: &Path, root: &Root, interactive: bool) -> Result<Self> {
        let name = Self::dir_name(target, root);
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

    /// A sensible default site name from the target directory's own name. A
    /// bare `.` resolves against the project root, so it yields the launch
    /// directory's name rather than an empty string.
    fn dir_name(target: &Path, root: &Root) -> String {
        root.join(target)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
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
pub fn new_page(ui: &Ui, path: &Path, config: &Config) -> Result<()> {
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
        .apply(ui)?;
    ui.done(format_args!("created {}", Paths(&path.display().to_string())));
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
    /// default (git) without asking; otherwise ask, but only when the session is
    /// `interactive` (decided once, in `init`) — piped or CI input defaults to
    /// none, so a scaffold never blocks nor creates a repo unbidden.
    fn wanted(yes: bool, interactive: bool, explicit: Option<Vcs>) -> Result<Option<Vcs>> {
        if explicit.is_some() {
            return Ok(explicit);
        }
        if yes {
            return Ok(Some(Vcs::Git));
        }
        if !interactive {
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
    fn setup(&self, ui: &Ui) -> Result<()> {
        let ignore = self.root.join(".gitignore");
        if !ignore.exists() {
            fs::write(&ignore, Self::IGNORE)?;
            ui.detail(format_args!("{} {}", "+".green(), Paths(".gitignore")));
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
            Ok(out) if out.status.success() => {
                ui.detail(format_args!("{} {} repository", "+".green(), self.vcs.label()));
            }
            Ok(out) => {
                let detail = String::from_utf8_lossy(&out.stderr);
                let detail = detail.trim();
                ui.warn(VcsFailed {
                    tool: cmd,
                    detail: (!detail.is_empty()).then(|| detail.to_owned()),
                });
            }
            Err(_) => ui.warn(VcsMissing { tool: cmd }),
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

    /// Substitute `{{key}}` placeholders in a template, in a single left-to-
    /// right pass: a substituted value is never rescanned, so a site name
    /// containing `{{author}}` stays literal. Values are escaped for the
    /// double-quoted string context they land in (KDL and typst share `\`
    /// and `"` escapes), so a quote in a site name yields valid config.
    /// Unknown placeholders are left untouched.
    pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
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
            match vars.iter().find(|(k, _)| *k == key) {
                Some((_, value)) => out.push_str(&escape(value)),
                None => {
                    out.push_str("{{");
                    out.push_str(key);
                    out.push_str("}}");
                }
            }
            rest = &after[close + 2..];
        }
        out.push_str(rest);
        out
    }

    /// Escape a value for interpolation into a double-quoted string literal.
    fn escape(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    #[cfg(test)]
    mod tests {
        use super::render;

        #[test]
        fn fills_known_placeholders() {
            let out = render("site \"{{site}}\" by {{author}}", &[("site", "S"), ("author", "A")]);
            assert_eq!(out, "site \"S\" by A");
        }

        #[test]
        fn escapes_quotes_and_backslashes_in_values() {
            let out = render("site \"{{site}}\"", &[("site", "My \"Quoted\\\" Site")]);
            assert_eq!(out, "site \"My \\\"Quoted\\\\\\\" Site\"");
        }

        #[test]
        fn substituted_values_are_never_rescanned() {
            let out = render(
                "{{site}} by {{author}}",
                &[("site", "{{author}}"), ("author", "Me")],
            );
            assert_eq!(out, "{{author}} by Me");
        }

        #[test]
        fn unknown_placeholders_are_left_alone() {
            assert_eq!(render("keep {{unknown}}", &[("site", "S")]), "keep {{unknown}}");
        }

        #[test]
        fn unterminated_braces_pass_through() {
            assert_eq!(render("dangling {{site", &[("site", "S")]), "dangling {{site");
        }
    }
}
