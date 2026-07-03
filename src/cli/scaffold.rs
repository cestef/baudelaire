use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;

use crate::cli::output::Report;
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
                report.muted(format_args!("  {} {}", "+".green(), dir.display().dimmed()))?;
            }
        }
        for (rel, contents) in &self.files {
            let full = self.root.join(rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full, contents)?;
            report.muted(format_args!("  {} {}", "+".green(), rel.display().dimmed()))?;
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

/// Scaffold a new project.
pub fn init(report: &mut Report, root: &Path) -> Result<()> {
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
