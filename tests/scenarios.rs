//! Data-driven end-to-end builds.
//!
//! Every case under `tests/scenarios/*.kdl` is a set of files to lay down in a
//! tempdir and a set of claims about the site they build to. The site lives in
//! KDL — the language the product's own config speaks — so a config change is a
//! mechanical edit of data files rather than of escaped Rust string literals.
//!
//! # The format
//!
//! ```kdl
//! scenario "a dated collection emits an RSS feed" {
//!   requires "images"                 // skip unless this cargo feature is on
//!   files {
//!     "config.kdl" #"""
//!       site "T"
//!       url "https://example.com"
//!       generate { feed { formats "rss" } }
//!       """#
//!     "content/posts/a.typ" #"""
//!       #let frontmatter = (title: "A", date: datetime(year: 2026, month: 1, day: 2))
//!       Hello.
//!       """#
//!     "assets/big.png" png="60x40"    // a generated image, for the pixels
//!   }
//!   expect {
//!     ok                              // ...or `fails { code "baudelaire::x" }`
//!     file "rss.xml" {
//!       contains "<title>A</title>"
//!       missing "<title></title>"
//!       order "first" "second"        // both present, in that order
//!     }
//!     file "assets/big.png"           // no claims: it merely has to exist
//!     absent "atom.xml"
//!   }
//! }
//! ```
//!
//! A file may hold any number of `scenario` nodes; group them by what they
//! prove. Paths in `files` are relative to the site root, paths in `expect` to
//! the configured `dist`. Multi-line raw strings (`#"""` … `"""#`) dedent to the
//! closing delimiter, so bodies need no escaping.
//!
//! Failure is matched on the miette diagnostic *code*, never on message text, so
//! rewording an error does not break a scenario.
//!
//! # What cannot become a scenario
//!
//! A scenario is exactly one in-process build of one tempdir, checked against
//! files on disk. Everything below falls outside that and stays in Rust:
//!
//! | Kind | Where | Why |
//! | --- | --- | --- |
//! | live server, SSE, timing | `serve_e2e.rs` | a running process, not a build |
//! | multi-build cache sequences | `incremental_e2e.rs` | build, edit, rebuild, compare |
//! | subprocess + exit codes + stderr | `cli_e2e.rs`, and the `Site::build` users in `build_e2e.rs` | the CLI's own output, not the site's |
//! | in-process SSH server | `deploy_e2e.rs` | a fixture the harness has to host |
//! | in-process HTTP server | `external_e2e.rs` | ditto |
//! | project generation | `scaffold_e2e.rs` | there is no site to lay down yet |
//! | library-level calls | `discovery.rs`, `frontmatter.rs` | assert on Rust values, not files |
//! | bundled JS assets | `virtual_modules_e2e.rs` | reads the binary's own resources |
//! | cargo feature matrix | `features_e2e.rs` | asserts on what a build *cannot* do |
//!
//! Two narrower categories cut across the files that *are* mostly migrated:
//!
//! - **Capture-then-compare.** Fingerprinting renames `style.css` to
//!   `style.<hash>.css`, and the assertion is that the *page* references that
//!   same generated name. A scenario has no way to bind a name it did not write.
//! - **Byte-level claims.** "the optimized PNG is smaller than its source", "an
//!   unchanged image is not re-encoded", "the copied bytes are the source
//!   bytes": the vocabulary here is text claims about text files.

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use kdl::{KdlDocument, KdlNode};
use miette::Diagnostic;

use common::Site;

/// The cargo features a scenario may name in `requires`, paired with whether
/// this build has them. The single list: `requires` both validates and skips
/// against it, so a typo is a hard error rather than a case that quietly never
/// runs.
const FEATURES: &[(&str, bool)] = &[
    ("announce", cfg!(feature = "announce")),
    ("cards", cfg!(feature = "cards")),
    ("css", cfg!(feature = "css")),
    ("images", cfg!(feature = "images")),
    ("js", cfg!(feature = "js")),
];

/// What to write for one input file.
enum Source {
    /// Written verbatim, with a trailing newline: config, `.typ`, `.css`, `.js`.
    Text(String),
    /// A generated `WxH` PNG whose pixels vary with position, so two sizes never
    /// share bytes. Images need real dimensions an encoder agrees with; a text
    /// placeholder will not do.
    Png(u32, u32),
}

impl Source {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        if let Some(dims) = node.get("png").and_then(|v| v.as_string()) {
            let (w, h) = dims
                .split_once('x')
                .ok_or_else(|| format!("`png={dims}` is not `<width>x<height>`"))?;
            let dim = |s: &str| {
                s.parse::<u32>()
                    .map_err(|_| format!("`png={dims}` is not `<width>x<height>`"))
            };
            return Ok(Self::Png(dim(w)?, dim(h)?));
        }
        let body = node
            .entries()
            .first()
            .filter(|e| e.name().is_none())
            .and_then(|e| e.value().as_string())
            .ok_or("a file needs a body string or a `png=WxH`")?;
        Ok(Self::Text(format!("{}\n", body.trim_end_matches('\n'))))
    }

    fn write(&self, site: &Site, path: &str) {
        match self {
            Self::Text(body) => site.write(path, body),
            Self::Png(w, h) => site.write_bytes(path, &Self::png(*w, *h)),
        }
    }

    /// A PNG of the given size, its pixels varying with position.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x * 7 + y * 13) as u8, (x * 3) as u8, (y * 5) as u8])
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("encode png");
        buf.into_inner()
    }
}

/// One claim about the text of a built file.
enum Claim {
    Contains(String),
    Missing(String),
    Equals(String),
    /// Every needle appears, each after the one before it.
    Order(Vec<String>),
}

impl Claim {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        let args = || -> Result<Vec<String>, String> {
            let args: Vec<_> = node
                .entries()
                .iter()
                .filter(|e| e.name().is_none())
                .filter_map(|e| e.value().as_string().map(str::to_owned))
                .collect();
            if args.is_empty() {
                return Err(format!("`{}` needs at least one string", node.name()));
            }
            Ok(args)
        };
        let one = || -> Result<String, String> {
            let mut args = args()?;
            match args.len() {
                1 => Ok(args.remove(0)),
                n => Err(format!("`{}` takes one string, got {n}", node.name())),
            }
        };
        match node.name().value() {
            "contains" => Ok(Self::Contains(one()?)),
            "missing" => Ok(Self::Missing(one()?)),
            "equals" => Ok(Self::Equals(one()?)),
            "order" => Ok(Self::Order(args()?)),
            other => Err(format!("unknown claim `{other}`")),
        }
    }

    /// The complaint this claim has about `text`, if any.
    fn check(&self, text: &str) -> Option<String> {
        match self {
            Self::Contains(needle) if !text.contains(needle.as_str()) => {
                Some(format!("does not contain {needle:?}"))
            }
            Self::Missing(needle) if text.contains(needle.as_str()) => {
                Some(format!("still contains {needle:?}"))
            }
            Self::Equals(want) if text.trim_end() != want.trim_end() => {
                Some(format!("is {:?}, not {want:?}", text.trim_end()))
            }
            Self::Order(needles) => {
                let mut at = 0;
                for needle in needles {
                    match text[at..].find(needle.as_str()) {
                        Some(hit) => at += hit + needle.len(),
                        None => {
                            return Some(format!(
                                "does not contain {needle:?} after the needles before it"
                            ));
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }
}

/// A built file and everything claimed about it. No claims means the file only
/// has to exist, which is also how a binary output is asserted.
struct Output {
    path: String,
    claims: Vec<Claim>,
}

impl Output {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        let path = string_arg(node)?;
        let claims = node
            .iter_children()
            .map(Claim::parse)
            .collect::<Result<_, _>>()?;
        Ok(Self { path, claims })
    }

    fn check(&self, dist: &Path, into: &mut Vec<String>) {
        let full = dist.join(&self.path);
        if !full.exists() {
            into.push(format!("{}: not built", self.path));
            return;
        }
        if self.claims.is_empty() {
            return;
        }
        let bytes = std::fs::read(&full).expect("read output");
        let text = String::from_utf8_lossy(&bytes);
        for claim in &self.claims {
            if let Some(complaint) = claim.check(&text) {
                into.push(format!(
                    "{}: {complaint}\n{}",
                    self.path,
                    Self::excerpt(&text)
                ));
            }
        }
    }

    /// The head of a file, for a failure message: enough to see what went wrong
    /// without pasting a whole bundled stylesheet into the report.
    fn excerpt(text: &str) -> String {
        const LIMIT: usize = 2000;
        let head: String = text.chars().take(LIMIT).collect();
        if text.chars().nth(LIMIT).is_some() {
            format!("{head}…")
        } else {
            head
        }
    }
}

/// Whether the build is meant to succeed, and if not, with which diagnostic.
enum Outcome {
    Ok,
    /// The miette code of the expected error, e.g. `baudelaire::svg::malformed`.
    Fails(String),
}

impl Outcome {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        match node.name().value() {
            "ok" => Ok(Self::Ok),
            "fails" => {
                let code = node
                    .iter_children()
                    .find(|n| n.name().value() == "code")
                    .ok_or("`fails` needs a `code \"baudelaire::…\"`")?;
                Ok(Self::Fails(string_arg(code)?))
            }
            other => Err(format!("`{other}` is not an outcome")),
        }
    }

    /// The complaints this outcome has about what the build actually did. A
    /// build that failed as intended stops here: there is no output to inspect.
    fn check(&self, result: baudelaire::Result<baudelaire::engine::Stats>) -> Vec<String> {
        match (self, result) {
            (Self::Ok, Ok(_)) => Vec::new(),
            (Self::Ok, Err(err)) => vec![format!("the build failed:\n{}", Diagnosis(&err))],
            (Self::Fails(code), Ok(_)) => {
                vec![format!("the build succeeded, expected `{code}`")]
            }
            (Self::Fails(want), Err(err)) => {
                let got = err.code().map_or_else(String::new, |c| c.to_string());
                if &got == want {
                    Vec::new()
                } else {
                    vec![format!(
                        "failed with `{got}`, expected `{want}`:\n{}",
                        Diagnosis(&err)
                    )]
                }
            }
        }
    }
}

/// Everything a scenario claims about the site it just built.
struct Expect {
    outcome: Outcome,
    outputs: Vec<Output>,
    absent: Vec<String>,
}

impl Expect {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        let mut outcome = None;
        let mut outputs = Vec::new();
        let mut absent = Vec::new();
        for child in node.iter_children() {
            match child.name().value() {
                "file" => outputs.push(Output::parse(child)?),
                "absent" => absent.push(string_arg(child)?),
                _ => {
                    if outcome.replace(Outcome::parse(child)?).is_some() {
                        return Err("`expect` takes one outcome".into());
                    }
                }
            }
        }
        Ok(Self {
            outcome: outcome.ok_or("`expect` needs `ok` or `fails { }`")?,
            outputs,
            absent,
        })
    }
}

/// One case: the files to lay down, and what building them must produce.
struct Scenario {
    name: String,
    /// Cargo features without which this case is skipped.
    requires: Vec<String>,
    files: Vec<(String, Source)>,
    expect: Expect,
}

impl Scenario {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        let name = string_arg(node)?;
        let mut requires = Vec::new();
        let mut files = Vec::new();
        let mut expect = None;
        for child in node.iter_children() {
            match child.name().value() {
                "requires" => {
                    let feature = string_arg(child)?;
                    if !FEATURES.iter().any(|(name, _)| *name == feature) {
                        return Err(format!("`{feature}` is not a gating cargo feature"));
                    }
                    requires.push(feature);
                }
                "files" => {
                    for file in child.iter_children() {
                        files.push((file.name().value().to_owned(), Source::parse(file)?));
                    }
                }
                "expect" => expect = Some(Expect::parse(child)?),
                other => return Err(format!("unknown scenario section `{other}`")),
            }
        }
        if !files.iter().any(|(path, _)| path == "config.kdl") {
            return Err("a scenario needs a `config.kdl`".into());
        }
        Ok(Self {
            name,
            requires,
            files,
            expect: expect.ok_or("a scenario needs an `expect { }`")?,
        })
    }

    /// Whether this build has every feature the case needs.
    fn enabled(&self) -> bool {
        self.requires
            .iter()
            .all(|want| FEATURES.iter().any(|(name, on)| name == want && *on))
    }

    /// Build the case in a fresh tempdir and collect every unmet expectation.
    fn run(&self) -> Vec<String> {
        let site = Site::new();
        for (path, source) in &self.files {
            source.write(&site, path);
        }

        let mut failures = self.expect.outcome.check(site.try_build());
        if !failures.is_empty() || matches!(self.expect.outcome, Outcome::Fails(_)) {
            return failures;
        }

        let dist = site.config().paths.dist;
        for output in &self.expect.outputs {
            output.check(&dist, &mut failures);
        }
        for path in &self.expect.absent {
            if dist.join(path).exists() {
                failures.push(format!("{path}: built, but expected absent"));
            }
        }
        failures
    }
}

/// A scenario together with the file it came from, which is how it is named in
/// the report.
struct Case {
    suite: String,
    scenario: Scenario,
}

impl Case {
    /// Every case under `tests/scenarios/`, in a stable order.
    fn load_all() -> Vec<Self> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios");
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "kdl"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "no scenarios in {}", dir.display());
        paths.iter().flat_map(|path| Self::load(path)).collect()
    }

    fn load(path: &Path) -> Vec<Self> {
        let suite = path
            .file_stem()
            .expect("named file")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(path).expect("read scenario");
        let doc: KdlDocument = text
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        doc.nodes()
            .iter()
            .map(|node| {
                assert_eq!(
                    node.name().value(),
                    "scenario",
                    "{}: only `scenario` nodes belong at the top level",
                    path.display()
                );
                let scenario = Scenario::parse(node)
                    .unwrap_or_else(|e| panic!("{}:{}: {e}", path.display(), line_of(&text, node)));
                Self {
                    suite: suite.clone(),
                    scenario,
                }
            })
            .collect()
    }

    fn id(&self) -> String {
        format!("{} :: {}", self.suite, self.scenario.name)
    }
}

/// Runs every scenario and reports *all* the failures, not the first: one bad
/// case hiding fifty others is worse than no harness at all.
#[test]
fn scenarios() {
    let cases: Vec<_> = Case::load_all()
        .into_iter()
        .filter(|case| case.scenario.enabled())
        .collect();
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(cases.len()));

    let reports: Vec<Vec<(String, Vec<String>)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    while let Some(case) = cases.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let failures = case.scenario.run();
                        if !failures.is_empty() {
                            mine.push((case.id(), failures));
                        }
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut failed: Vec<_> = reports.into_iter().flatten().collect();
    if failed.is_empty() {
        return;
    }
    failed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut report = format!("{} of {} scenarios failed:\n", failed.len(), cases.len());
    for (id, failures) in &failed {
        let _ = writeln!(report, "\n  {id}");
        for failure in failures {
            let _ = writeln!(report, "    - {}", failure.replace('\n', "\n      "));
        }
    }
    panic!("{report}");
}

/// The sole string argument of a node, e.g. the path of `file "index.html"`.
fn string_arg(node: &KdlNode) -> Result<String, String> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_string())
        .map(str::to_owned)
        .ok_or_else(|| format!("`{}` needs a string argument", node.name()))
}

/// The 1-based line a node starts on, for pointing at a malformed scenario.
fn line_of(text: &str, node: &KdlNode) -> usize {
    text[..node.span().offset().min(text.len())]
        .lines()
        .count()
        .max(1)
}

/// A diagnostic as `code: message`, with its related diagnostics indented under
/// it: enough to see which page failed to compile and why, without pulling in
/// miette's fancy renderer and its colors.
struct Diagnosis<'a>(&'a dyn Diagnostic);

impl std::fmt::Display for Diagnosis<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = self.0.code() {
            write!(f, "{code}: ")?;
        }
        write!(f, "{}", self.0)?;
        for related in self.0.related().into_iter().flatten() {
            write!(
                f,
                "\n  {}",
                Diagnosis(related).to_string().replace('\n', "\n  ")
            )?;
        }
        Ok(())
    }
}
