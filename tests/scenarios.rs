//! Data-driven end-to-end builds: every case under `tests/scenarios/*.kdl` is a
//! set of files to lay down in a tempdir and a set of claims about what they
//! build to.
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
//! Paths in `files` are relative to the site root, paths in `expect` to the
//! configured `dist`. Failure, `warns`, `advises` and `unwarned` all match on
//! the miette diagnostic *code*, never on message text. A `defaults { files }`
//! node at the top of a file is laid down before every scenario in it, path by
//! path.
//!
//! `expect { }` is one build; a case needing more writes a `build { }` (or
//! `check { }`) node per run, each optionally editing the site first:
//!
//! ```kdl
//! scenario "an edited page rebuilds alone" {
//!   files { "config.kdl" ..; "content/a.typ" ..; "content/b.typ" .. }
//!   build { expect { ok; pages 2; cached 0 } }
//!   build {
//!     files { "content/a.typ" #"edited"# }   // written before this run
//!     remove "content/c.typ"                 // deleted before it
//!     expect { ok; pages 2; cached 1 }
//!   }
//! }
//! ```
//!
//! A step also carries the overrides a command line would pass: `drafts`,
//! `future`, `base "https://.."`, `profile "dev"`, each a toggle.
//!
//! `capture` binds part of a built file to a scenario-wide name, and `{name}`
//! interpolates it into any later path or needle, never into a `matches` or
//! `capture` pattern, where `{8}` is a quantifier. A `file`/`absent` path may
//! hold a `*`, which matches within one segment and must hit exactly one file:
//!
//! ```kdl
//! file "assets/style.*.css" { capture "bg" #"/assets/(bg\.[0-9a-f]+\.png)"# }
//! file "assets/{bg}"
//! ```

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use kdl::{KdlNode, KdlValue};
use miette::Diagnostic;

use baudelaire::config::Config;
use baudelaire::engine::Mode;
use baudelaire::ui::Bytes;
use common::{Run, Site};

/// The cargo features a scenario may name in `requires`, paired with whether
/// this build has them, read from the binary's own inventory so that a typo is
/// a hard error rather than a case that quietly never runs.
const FEATURES: &[(&str, bool)] = baudelaire::version::Version::FEATURES;

/// Names bound by a `capture` claim, and the `{name}` substitution they drive.
///
/// Scenario-wide and ordered, so a step can assert against a name an earlier
/// step bound.
#[derive(Default)]
struct Bindings(BTreeMap<String, String>);

impl Bindings {
    fn bind(&mut self, name: &str, value: &str) {
        self.0.insert(name.to_owned(), value.to_owned());
    }

    /// `text` with every `{name}` replaced by what it captured; an unbound name
    /// is an error rather than a literal.
    fn expand(&self, text: &str) -> Result<String, String> {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                out.push_str(&rest[start..]);
                return Ok(out);
            };
            let name = &after[..end];
            let value = self.0.get(name).ok_or_else(|| {
                format!(
                    "`{{{name}}}` is not bound; captured so far: {}",
                    if self.0.is_empty() {
                        "nothing".to_owned()
                    } else {
                        self.0.keys().cloned().collect::<Vec<_>>().join(", ")
                    }
                )
            })?;
            out.push_str(value);
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

/// What to write for one input file.
#[derive(Clone)]
enum Source {
    /// Written verbatim, with a trailing newline: config, `.typ`, `.css`.
    Text(String),
    /// A generated `WxH` image whose pixels vary with position, so two sizes
    /// never share bytes.
    Image(image::ImageFormat, u32, u32),
    /// Bytes settled at parse time: a fixture copied out of the repo (`from=`),
    /// or a short literal (`bytes=` hex).
    Bytes(Vec<u8>),
}

impl Source {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        if let Some(dims) = node.get("png").and_then(KdlValue::as_string) {
            let (w, h) = Self::dimensions(dims)?;
            return Ok(Self::Image(image::ImageFormat::Png, w, h));
        }
        if let Some(dims) = node.get("jpeg").and_then(KdlValue::as_string) {
            let (w, h) = Self::dimensions(dims)?;
            return Ok(Self::Image(image::ImageFormat::Jpeg, w, h));
        }
        if let Some(from) = node.get("from").and_then(KdlValue::as_string) {
            // Read now, so a mistyped fixture path fails every run rather
            // than one case somewhere.
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(from);
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("`from={from}`: {} ({e})", path.display()))?;
            return Ok(Self::Bytes(bytes));
        }
        if let Some(hex) = node.get("bytes").and_then(KdlValue::as_string) {
            return Self::hex(hex).map(Self::Bytes);
        }
        let body = node
            .entries()
            .first()
            .filter(|e| e.name().is_none())
            .and_then(|e| e.value().as_string())
            .ok_or("a file needs a body string, a `png=`/`jpeg=WxH`, a `from=`, or a `bytes=`")?;
        Ok(Self::Text(format!("{}\n", body.trim_end_matches('\n'))))
    }

    fn dimensions(dims: &str) -> Result<(u32, u32), String> {
        let bad = || format!("`{dims}` is not `<width>x<height>`");
        let (w, h) = dims.split_once('x').ok_or_else(bad)?;
        let dim = |s: &str| s.parse::<u32>().map_err(|_| bad());
        Ok((dim(w)?, dim(h)?))
    }

    fn hex(text: &str) -> Result<Vec<u8>, String> {
        let digits: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        if !digits.len().is_multiple_of(2) {
            return Err(format!("`bytes={text}` has an odd number of hex digits"));
        }
        digits
            .chunks(2)
            .map(|pair| {
                let byte: String = pair.iter().collect();
                u8::from_str_radix(&byte, 16).map_err(|_| format!("`{byte}` is not a hex byte"))
            })
            .collect()
    }

    fn write(&self, site: &Site, path: &str) {
        match self {
            Self::Text(body) => site.write(path, body),
            Self::Image(format, w, h) => site.write_bytes(path, &Self::image(*format, *w, *h)),
            Self::Bytes(bytes) => site.write_bytes(path, bytes),
        }
    }

    /// An image of the given size, its pixels varying with position.
    fn image(format: image::ImageFormat, w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            // The pattern is meant to wrap, so a truncating cast is the point.
            image::Rgb([
                ((x * 7 + y * 13) & 0xff) as u8,
                ((x * 3) & 0xff) as u8,
                ((y * 5) & 0xff) as u8,
            ])
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, format)
            .expect("encode image");
        buf.into_inner()
    }
}

/// A built file, as every claim sees it: its bytes, its text, and the site it
/// came from, so a claim comparing against an *input* can reach one.
struct Built<'a> {
    path: String,
    bytes: Vec<u8>,
    text: String,
    root: &'a Path,
}

impl Built<'_> {
    /// The bytes of one of the site's own input files, for the claims that
    /// compare an output against what produced it.
    fn source(&self, rel: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.root.join(rel)).map_err(|e| format!("cannot read input `{rel}`: {e}"))
    }
}

/// One claim about a built file. Text claims read the file as UTF-8 (lossily,
/// so a binary file still answers a size question); the rest work on bytes.
enum Claim {
    Contains(String),
    Missing(String),
    Equals(String),
    /// Every needle appears, each after the one before it.
    Order(Vec<String>),
    /// A needle appears exactly this many times, which `contains` cannot say.
    Count(String, usize),
    Matches(regex::Regex),
    /// Bind what the pattern matched (group 1 if it has one, else the whole
    /// match) to a name, for `{name}` in a later path or needle.
    Capture(String, regex::Regex),
    /// A byte-size bound, written in bytes or in the units the build summary
    /// prints. At least one end is set.
    Size {
        min: Option<u64>,
        max: Option<u64>,
    },
    /// Byte-identical to one of the site's inputs: "copied, not processed".
    Identical(String),
    /// Strictly fewer bytes than one of the site's inputs: "optimized".
    Smaller(String),
    /// A claim about the value at a JSON pointer, for the generated documents
    /// whose shape matters more than their text (search index, feed, report).
    Json(String, Json),
}

/// What a `json` claim says about the value it addressed.
enum Json {
    /// It is there at all.
    Exists,
    /// It equals this, compared as a bare string for a JSON string and as
    /// compact JSON for anything else.
    Equals(String),
    /// It is an array or object of this size.
    Count(usize),
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
        let two = || -> Result<(String, String), String> {
            let mut args = args()?;
            match args.len() {
                2 => Ok((args.remove(0), args.remove(0))),
                n => Err(format!("`{}` takes two strings, got {n}", node.name())),
            }
        };
        let regex = |pattern: &str| {
            regex::Regex::new(pattern).map_err(|e| format!("`{pattern}` is not a regex: {e}"))
        };
        match node.name().value() {
            "contains" => Ok(Self::Contains(one()?)),
            "missing" => Ok(Self::Missing(one()?)),
            "equals" => Ok(Self::Equals(one()?)),
            "order" => Ok(Self::Order(args()?)),
            "count" => {
                let needle = one()?;
                let times = number(node, "count").ok_or("`count` needs an integer `n`")?;
                Ok(Self::Count(needle, times))
            }
            "matches" => Ok(Self::Matches(regex(&one()?)?)),
            "capture" => {
                let (name, pattern) = two()?;
                Ok(Self::Capture(name, regex(&pattern)?))
            }
            "size" => {
                let bound = |key| -> Result<Option<u64>, String> {
                    node.get(key).map_or(Ok(None), |value| {
                        size(value)
                            .map(Some)
                            .ok_or_else(|| format!("`size {key}=` is not a byte size"))
                    })
                };
                let (min, max) = (bound("min")?, bound("max")?);
                if min.is_none() && max.is_none() {
                    Err("`size` needs a `min=` or a `max=`".into())
                } else {
                    Ok(Self::Size { min, max })
                }
            }
            "identical" => Ok(Self::Identical(one()?)),
            "smaller" => Ok(Self::Smaller(one()?)),
            "json" => {
                let pointer = one()?;
                let what = match (node.get("equals"), number(node, "count")) {
                    (Some(_), Some(_)) => {
                        return Err("`json` takes `equals=` or `count=`, not both".into());
                    }
                    (Some(value), None) => Json::Equals(
                        value
                            .as_string()
                            .map_or_else(|| value.to_string(), str::to_owned),
                    ),
                    (None, Some(n)) => Json::Count(n),
                    (None, None) => Json::Exists,
                };
                Ok(Self::Json(pointer, what))
            }
            other => Err(format!("unknown claim `{other}`")),
        }
    }

    /// The complaint this claim has about the file, if any. `binds` is mutated
    /// by `capture`, which is what lets one claim name what an earlier found.
    fn check(&self, file: &Built, binds: &mut Bindings) -> Option<String> {
        let text = &file.text;
        match self {
            Self::Contains(needle) => {
                let needle = binds.expand(needle).ok()?;
                (!text.contains(&needle)).then(|| format!("does not contain {needle:?}"))
            }
            Self::Missing(needle) => {
                let needle = binds.expand(needle).ok()?;
                text.contains(&needle)
                    .then(|| format!("still contains {needle:?}"))
            }
            Self::Equals(want) => {
                let want = binds.expand(want).ok()?;
                (text.trim_end() != want.trim_end())
                    .then(|| format!("is {:?}, not {want:?}", text.trim_end()))
            }
            Self::Order(needles) => {
                let mut at = 0;
                for needle in needles {
                    let needle = binds.expand(needle).ok()?;
                    match text[at..].find(&needle) {
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
            Self::Count(needle, want) => {
                let needle = binds.expand(needle).ok()?;
                let got = text.matches(&needle).count();
                (got != *want).then(|| format!("contains {needle:?} {got} times, not {want}"))
            }
            Self::Matches(pattern) => {
                (!pattern.is_match(text)).then(|| format!("does not match /{}/", pattern.as_str()))
            }
            Self::Capture(name, pattern) => match pattern.captures(text) {
                Some(found) => {
                    // Group 1 when the pattern has one, the whole match else.
                    let hit = found.get(1).or_else(|| found.get(0))?;
                    binds.bind(name, hit.as_str());
                    None
                }
                None => Some(format!(
                    "does not match /{}/, so `{name}` binds nothing",
                    pattern.as_str()
                )),
            },
            Self::Size { min, max } => {
                let got = file.bytes.len() as u64;
                match (min, max) {
                    (Some(min), _) if got < *min => {
                        Some(format!("is {got} bytes, under the {min} claimed"))
                    }
                    (_, Some(max)) if got > *max => {
                        Some(format!("is {got} bytes, over the {max} claimed"))
                    }
                    _ => None,
                }
            }
            Self::Identical(input) => match file.source(input) {
                Err(complaint) => Some(complaint),
                Ok(bytes) => (bytes != file.bytes).then(|| {
                    format!(
                        "is {} bytes, not byte-identical to `{input}` ({} bytes)",
                        file.bytes.len(),
                        bytes.len()
                    )
                }),
            },
            Self::Smaller(input) => match file.source(input) {
                Err(complaint) => Some(complaint),
                Ok(bytes) => (file.bytes.len() >= bytes.len()).then(|| {
                    format!(
                        "is {} bytes, not smaller than `{input}` ({} bytes)",
                        file.bytes.len(),
                        bytes.len()
                    )
                }),
            },
            Self::Json(pointer, what) => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
                    return Some("is not JSON".to_owned());
                };
                let Some(found) = value.pointer(pointer) else {
                    return Some(format!("has nothing at `{pointer}`"));
                };
                what.check(pointer, found, binds)
            }
        }
    }
}

impl Json {
    fn check(&self, pointer: &str, found: &serde_json::Value, binds: &Bindings) -> Option<String> {
        match self {
            Self::Exists => None,
            Self::Equals(want) => {
                let want = binds.expand(want).ok()?;
                // A JSON string compares as its bare content, so a claim reads
                // `equals="A"`; anything else compares as compact JSON.
                let got = found
                    .as_str()
                    .map_or_else(|| found.to_string(), std::borrow::ToOwned::to_owned);
                (got != want).then(|| format!("has {got:?} at `{pointer}`, not {want:?}"))
            }
            Self::Count(want) => {
                let got = match found {
                    serde_json::Value::Array(items) => items.len(),
                    serde_json::Value::Object(entries) => entries.len(),
                    _ => return Some(format!("has no length at `{pointer}`")),
                };
                (got != *want).then(|| format!("holds {got} at `{pointer}`, not {want}"))
            }
        }
    }
}

/// An output path, either literal or holding a `*` that matches within one
/// segment.
///
/// A pattern is the entry point `capture` needs when *nothing* names the file.
/// Exactly one file may match, since "one of these exists" is a claim a case
/// could pass by accident.
struct Pattern(String);

impl Pattern {
    fn wild(&self) -> bool {
        self.0.contains('*')
    }

    /// Whether a dist-relative path matches, segment by segment. Only `*` is
    /// special, and it never crosses a `/`.
    fn matches(&self, path: &str) -> bool {
        let (mut pattern, mut candidate) = (self.0.split('/'), path.split('/'));
        loop {
            match (pattern.next(), candidate.next()) {
                (None, None) => return true,
                (Some(want), Some(got)) if Self::segment(want, got) => {}
                _ => return false,
            }
        }
    }

    fn segment(pattern: &str, name: &str) -> bool {
        let mut at = 0;
        let mut parts = pattern.split('*');
        let Some(first) = parts.next() else {
            return true;
        };
        if !name[at..].starts_with(first) {
            return false;
        }
        at += first.len();
        let mut last: Option<&str> = None;
        for part in parts {
            match name[at..].find(part) {
                Some(hit) => at += hit + part.len(),
                None => return false,
            }
            last = Some(part);
        }
        // A trailing literal has to end the name, or `style.*.css` would match
        // `style.abc.css.map`.
        match last {
            Some(tail) if !tail.is_empty() => name.ends_with(tail),
            _ => true,
        }
    }

    /// Every dist-relative path under `dist` that matches.
    fn find(&self, dist: &Path) -> Vec<String> {
        let mut found = Vec::new();
        Self::walk(dist, dist, &mut found);
        found.retain(|path| self.matches(path));
        found.sort();
        found
    }

    fn walk(dist: &Path, dir: &Path, into: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                Self::walk(dist, &path, into);
            } else if let Ok(rel) = path.strip_prefix(dist) {
                into.push(rel.to_string_lossy().replace('\\', "/"));
            }
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

    fn check(&self, site: &Site, dist: &Path, binds: &mut Bindings, into: &mut Vec<String>) {
        let path = match binds.expand(&self.path) {
            Ok(path) => path,
            Err(complaint) => return into.push(format!("{}: {complaint}", self.path)),
        };
        let pattern = Pattern(path);
        let path = if pattern.wild() {
            match pattern.find(dist).as_slice() {
                [only] => only.clone(),
                [] => return into.push(format!("{}: nothing matches", pattern.0)),
                many => {
                    return into.push(format!(
                        "{}: {} files match: {}",
                        pattern.0,
                        many.len(),
                        many.join(", ")
                    ));
                }
            }
        } else {
            pattern.0
        };
        let full = dist.join(&path);
        if !full.exists() {
            into.push(format!("{path}: not built"));
            return;
        }
        if self.claims.is_empty() {
            return;
        }
        let bytes = std::fs::read(&full).expect("read output");
        let file = Built {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            path,
            bytes,
            root: &site.root,
        };
        for claim in &self.claims {
            if let Some(complaint) = claim.check(&file, binds) {
                into.push(format!(
                    "{}: {complaint}\n{}",
                    file.path,
                    Self::excerpt(&file.text)
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
    /// The miette code of the error, e.g. `baudelaire::svg::malformed`.
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

    /// The complaints this outcome has about what the build actually did.
    fn check(&self, result: &baudelaire::Result<baudelaire::engine::Stats>) -> Vec<String> {
        match (self, result) {
            (Self::Ok, Ok(_)) => Vec::new(),
            (Self::Ok, Err(err)) => vec![format!("the build failed:\n{}", Diagnosis(err))],
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
                        Diagnosis(err)
                    )]
                }
            }
        }
    }

    /// Whether a build that ended this way has output worth inspecting.
    fn built(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

/// Everything a scenario claims about one run and the site it left behind.
struct Expect {
    outcome: Outcome,
    /// Pages built and pages reused, when the case cares. `None` is "no claim",
    /// which is a different thing from claiming zero.
    pages: Option<usize>,
    cached: Option<usize>,
    /// Warning codes the run must have reported. Extra warnings are allowed:
    /// a case names the one it is about, not every one the build may add.
    warns: Vec<String>,
    /// Advice codes the run must have reported. Its own claim rather than part
    /// of `warns`, because advice never counts against a build.
    advises: Vec<String>,
    /// Diagnostic codes that must *not* be reported: the claim that a check
    /// stayed quiet about a page.
    unwarned: Vec<String>,
    outputs: Vec<Output>,
    absent: Vec<String>,
}

impl Expect {
    fn parse(node: &KdlNode) -> Result<Self, String> {
        let mut expect = Self {
            outcome: Outcome::Ok,
            pages: None,
            cached: None,
            warns: Vec::new(),
            advises: Vec::new(),
            unwarned: Vec::new(),
            outputs: Vec::new(),
            absent: Vec::new(),
        };
        let mut outcome = None;
        for child in node.iter_children() {
            let name = child.name().value();
            match name {
                "file" => expect.outputs.push(Output::parse(child)?),
                "absent" => expect.absent.push(string_arg(child)?),
                "warns" => expect.warns.push(string_arg(child)?),
                "advises" => expect.advises.push(string_arg(child)?),
                "unwarned" => expect.unwarned.push(string_arg(child)?),
                "pages" | "cached" => {
                    let count = number(child, "n")
                        .or_else(|| {
                            child
                                .get(0)
                                .and_then(kdl::KdlValue::as_integer)
                                .and_then(count)
                        })
                        .ok_or_else(|| format!("`{name}` needs a count"))?;
                    match name {
                        "pages" => expect.pages = Some(count),
                        _ => expect.cached = Some(count),
                    }
                }
                _ => {
                    if outcome.replace(Outcome::parse(child)?).is_some() {
                        return Err("`expect` takes one outcome".into());
                    }
                }
            }
        }
        expect.outcome = outcome.ok_or("`expect` needs `ok` or `fails { }`")?;
        Ok(expect)
    }

    /// Every unmet claim about `run`, and about the files it left in `dist`.
    fn check(&self, site: &Site, dist: &Path, run: &Run, binds: &mut Bindings) -> Vec<String> {
        let mut failures = self.outcome.check(&run.result);
        let reported = run.warnings();
        for want in &self.warns {
            if !reported.iter().any(|got| got == want) {
                failures.push(if reported.is_empty() {
                    format!("expected warning `{want}`, but nothing warned")
                } else {
                    format!("expected warning `{want}`, got: {}", reported.join(", "))
                });
            }
        }
        let advised = run.advice();
        for want in &self.advises {
            if !advised.iter().any(|got| got == want) {
                failures.push(if advised.is_empty() {
                    format!("expected advice `{want}`, but nothing advised")
                } else {
                    format!("expected advice `{want}`, got: {}", advised.join(", "))
                });
            }
        }
        for unwanted in &self.unwarned {
            if reported.iter().any(|got| got == unwanted) {
                failures.push(format!("warned `{unwanted}`, expected silence about it"));
            }
        }
        if let Ok(stats) = &run.result {
            let counted = |what, want: Option<usize>, got: usize| match want {
                Some(want) if want != got => Some(format!("{what}: {got}, expected {want}")),
                _ => None,
            };
            failures.extend(counted("pages built", self.pages, stats.pages));
            failures.extend(counted("pages reused", self.cached, stats.cached));
        }
        // A build that failed as intended left no output to inspect, and one
        // that failed unintentionally has already said so.
        if !failures.is_empty() || !self.outcome.built() {
            return failures;
        }
        for output in &self.outputs {
            output.check(site, dist, binds, &mut failures);
        }
        for path in &self.absent {
            let found = match binds.expand(path).map(Pattern) {
                Err(complaint) => {
                    failures.push(format!("{path}: {complaint}"));
                    continue;
                }
                Ok(pattern) if pattern.wild() => pattern.find(dist),
                Ok(pattern) => {
                    if dist.join(&pattern.0).exists() {
                        vec![pattern.0]
                    } else {
                        Vec::new()
                    }
                }
            };
            if !found.is_empty() {
                failures.push(format!("{}: built, but expected absent", found.join(", ")));
            }
        }
        failures
    }
}

/// The command-line overrides a step applies over the config it loaded, each a
/// toggle so a step can turn *off* what the config turned on.
#[derive(Default)]
struct Knobs {
    drafts: Option<bool>,
    future: Option<bool>,
    strict: Option<bool>,
    cache: Option<bool>,
    base: Option<String>,
    profile: Option<String>,
}

impl Knobs {
    /// Read one override off a step's child node. `None` means the node is not
    /// an override at all, and the caller reads it as something else.
    fn read(&mut self, node: &KdlNode) -> Option<Result<(), String>> {
        let flag = || node.get(0).is_none_or(|v| v.as_bool().unwrap_or(true));
        let text = || string_arg(node);
        match node.name().value() {
            "drafts" => self.drafts = Some(flag()),
            "future" => self.future = Some(flag()),
            "strict-links" => self.strict = Some(flag()),
            "cache" => self.cache = Some(flag()),
            "base" => match text() {
                Ok(url) => self.base = Some(url),
                Err(e) => return Some(Err(e)),
            },
            "profile" => match text() {
                Ok(name) => self.profile = Some(name),
                Err(e) => return Some(Err(e)),
            },
            _ => return None,
        }
        Some(Ok(()))
    }

    /// Overlay onto a loaded config. The profile is applied first and by value,
    /// exactly as the CLI does, so a flag beats the profile it overlays.
    fn apply(&self, mut config: Config) -> baudelaire::Result<Config> {
        if let Some(profile) = &self.profile {
            config = config.with_profile(profile)?;
        }
        if let Some(url) = &self.base {
            config.url = Some(url.clone());
        }
        let set = |target: &mut bool, value: Option<bool>| {
            if let Some(value) = value {
                *target = value;
            }
        };
        set(&mut config.content.drafts.build, self.drafts);
        set(&mut config.content.future, self.future);
        if let Some(strict) = self.strict {
            config.check.links = baudelaire::config::Level::named(if strict {
                baudelaire::config::Severity::Error
            } else {
                baudelaire::config::Severity::Warn
            });
        }
        set(&mut config.cache.incremental, self.cache);
        Ok(config)
    }
}

/// One run of the engine: what to change about the site first, how to invoke
/// it, and what it must then be true of.
struct Step {
    mode: Mode,
    knobs: Knobs,
    /// Files written just before this run, over whatever is already there.
    files: Vec<(String, Source)>,
    /// Input paths deleted just before it.
    removes: Vec<String>,
    /// `None` for a warm-up run whose only job is to leave a cache behind.
    expect: Option<Expect>,
}

impl Step {
    fn parse(node: &KdlNode, mode: Mode) -> Result<Self, String> {
        let mut step = Self {
            mode,
            knobs: Knobs::default(),
            files: Vec::new(),
            removes: Vec::new(),
            expect: None,
        };
        for child in node.iter_children() {
            if let Some(result) = step.knobs.read(child) {
                result?;
                continue;
            }
            match child.name().value() {
                "files" => step.files.extend(laid_down(child)?),
                "remove" => step.removes.push(string_arg(child)?),
                "expect" => step.expect = Some(Expect::parse(child)?),
                other => return Err(format!("unknown step section `{other}`")),
            }
        }
        Ok(step)
    }

    /// Apply this step's edits, run it, and collect every unmet claim.
    fn run(&self, site: &Site, binds: &mut Bindings) -> Vec<String> {
        for (path, source) in &self.files {
            source.write(site, path);
        }
        for path in &self.removes {
            let full = site.path(path);
            let removed = if full.is_dir() {
                std::fs::remove_dir_all(&full)
            } else {
                std::fs::remove_file(&full)
            };
            if let Err(e) = removed {
                return vec![format!("cannot remove `{path}`: {e}")];
            }
        }
        let run = match site.try_config().and_then(|c| self.knobs.apply(c)) {
            Ok(config) => Run::of(config, self.mode),
            // A config this step cannot load is still an outcome the case may
            // have asked for.
            Err(err) => Run {
                result: Err(err),
                report: baudelaire::ui::Report {
                    schema: baudelaire::ui::Report::SCHEMA,
                    ok: false,
                    pages: None,
                    cached: None,
                    warnings: 0,
                    diagnostics: Vec::new(),
                },
            },
        };
        let Some(expect) = &self.expect else {
            return match &run.result {
                Err(err) => vec![format!("the build failed:\n{}", Diagnosis(err))],
                Ok(_) => Vec::new(),
            };
        };
        // Resolved after the run: a step may have edited `config.kdl` and moved
        // the output directory out from under the previous one.
        let dist = site
            .try_config()
            .map_or_else(|_| site.path("public"), |c| c.paths.dist);
        expect.check(site, &dist, &run, binds)
    }
}

/// One case: the files to lay down, and the runs that must then hold.
struct Scenario {
    name: String,
    /// Cargo features without which this case is skipped.
    requires: Vec<String>,
    files: Vec<(String, Source)>,
    steps: Vec<Step>,
}

impl Scenario {
    fn parse(node: &KdlNode, defaults: &[(String, Source)]) -> Result<Self, String> {
        let name = string_arg(node)?;
        let mut requires = Vec::new();
        let mut files = Vec::new();
        let mut steps = Vec::new();
        let mut sugar = false;
        for child in node.iter_children() {
            match child.name().value() {
                "requires" => {
                    let feature = string_arg(child)?;
                    if !FEATURES.iter().any(|(name, _)| *name == feature) {
                        return Err(format!("`{feature}` is not a gating cargo feature"));
                    }
                    requires.push(feature);
                }
                "files" => files.extend(laid_down(child)?),
                "build" => steps.push(Step::parse(child, Mode::Build)?),
                "check" => steps.push(Step::parse(child, Mode::Check)?),
                "expect" => {
                    // The one-build spelling is one `build { expect }`.
                    sugar = true;
                    steps.push(Step {
                        mode: Mode::Build,
                        knobs: Knobs::default(),
                        files: Vec::new(),
                        removes: Vec::new(),
                        expect: Some(Expect::parse(child)?),
                    });
                }
                other => return Err(format!("unknown scenario section `{other}`")),
            }
        }
        if sugar && steps.len() > 1 {
            return Err(
                "a bare `expect` is the one-build spelling: use `build { expect }` per run".into(),
            );
        }
        if steps.is_empty() {
            return Err("a scenario needs an `expect { }` or a `build { }`".into());
        }
        // Overlaid path by path, so naming `config.kdl` again replaces the
        // suite's and naming nothing keeps it.
        let laid = Self::overlay(defaults, files);
        if !laid.iter().any(|(path, _)| path == "config.kdl") {
            return Err("a scenario needs a `config.kdl`, its own or the suite's".into());
        }
        Ok(Self {
            name,
            requires,
            files: laid,
            steps,
        })
    }

    /// The suite defaults with the scenario's own files laid over them: a path
    /// in both keeps the scenario's, in declaration order with defaults first.
    fn overlay(defaults: &[(String, Source)], own: Vec<(String, Source)>) -> Vec<(String, Source)> {
        let mut out: Vec<(String, Source)> = Vec::new();
        for (path, source) in defaults {
            if own.iter().any(|(taken, _)| taken == path) {
                continue;
            }
            out.push((path.clone(), source.clone()));
        }
        out.extend(own);
        out
    }

    /// Whether this build has every feature the case needs.
    fn enabled(&self) -> bool {
        self.requires
            .iter()
            .all(|want| FEATURES.iter().any(|(name, on)| name == want && *on))
    }

    /// Lay the case down in a fresh tempdir, run every step in order, and
    /// collect every unmet expectation. A step that fails stops the case.
    fn run(&self) -> Vec<String> {
        let site = Site::new();
        for (path, source) in &self.files {
            source.write(&site, path);
        }
        let mut binds = Bindings::default();
        for (n, step) in self.steps.iter().enumerate() {
            let failures = step.run(&site, &mut binds);
            if !failures.is_empty() {
                return match self.steps.len() {
                    1 => failures,
                    _ => failures
                        .into_iter()
                        .map(|f| format!("step {}: {f}", n + 1))
                        .collect(),
                };
            }
        }
        Vec::new()
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
        let doc: kdl::KdlDocument = text
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let fail = |node: &KdlNode, e: String| -> ! {
            panic!("{}:{}: {e}", path.display(), line_of(&text, node))
        };
        let mut defaults: Vec<(String, Source)> = Vec::new();
        let mut cases = Vec::new();
        for node in doc.nodes() {
            match node.name().value() {
                "defaults" => {
                    for child in node.iter_children() {
                        match child.name().value() {
                            "files" => match laid_down(child) {
                                Ok(found) => defaults.extend(found),
                                Err(e) => fail(child, e),
                            },
                            other => fail(child, format!("unknown defaults section `{other}`")),
                        }
                    }
                }
                "scenario" => match Scenario::parse(node, &defaults) {
                    Ok(scenario) => cases.push(Self {
                        suite: suite.clone(),
                        scenario,
                    }),
                    Err(e) => fail(node, e),
                },
                other => fail(
                    node,
                    format!("`{other}`: only `defaults` and `scenario` belong at the top level"),
                ),
            }
        }
        cases
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

/// The `(path, source)` pairs of a `files { }` block.
fn laid_down(node: &KdlNode) -> Result<Vec<(String, Source)>, String> {
    node.iter_children()
        .map(|file| Ok((file.name().value().to_owned(), Source::parse(file)?)))
        .collect()
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

/// A named integer attribute (`count "x" n=2`), or the node's own trailing
/// integer argument (`count "x" 2`), whichever the case wrote.
fn number(node: &KdlNode, key: &str) -> Option<usize> {
    let named = node.get(key).and_then(kdl::KdlValue::as_integer);
    let positional = node
        .entries()
        .iter()
        .filter(|e| e.name().is_none())
        .find_map(|e| e.value().as_integer());
    named.or(positional).and_then(count)
}

fn count(n: i128) -> Option<usize> {
    usize::try_from(n).ok()
}

/// A byte size written as an integer of bytes or as a string with a unit, read
/// by the parser the config itself uses.
fn size(value: &KdlValue) -> Option<u64> {
    value.as_string().map_or_else(
        || value.as_integer().and_then(|n| u64::try_from(n).ok()),
        |text| Bytes::parse(text).map(|b| b.0),
    )
}

/// The 1-based line a node starts on, so a malformed case points at itself.
fn line_of(text: &str, node: &KdlNode) -> usize {
    let offset = node.span().offset();
    text[..offset.min(text.len())].lines().count().max(1)
}

/// The two pieces of the format that no scenario can prove about itself: path
/// matching, and the refusal to expand an unbound name.
#[test]
fn the_format_matches_paths_and_expands_names_as_documented() {
    let matches = |pattern: &str, path: &str| Pattern(pattern.to_owned()).matches(path);
    assert!(matches("assets/style.*.css", "assets/style.deadbeef.css"));
    assert!(matches("assets/*", "assets/style.css"));
    assert!(matches("index.html", "index.html"));
    // `*` stops at a separator, so a pattern names one file and never a tree.
    assert!(!matches("assets/*", "assets/deep/style.css"));
    // A trailing literal has to end the name.
    assert!(!matches(
        "assets/style.*.css",
        "assets/style.deadbeef.css.map"
    ));
    assert!(!matches("assets/style.*.css", "assets/other.deadbeef.css"));
    assert!(!matches("index.html", "posts/index.html"));

    let mut binds = Bindings::default();
    binds.bind("css", "style.abc.css");
    assert_eq!(
        binds.expand("assets/{css}").unwrap(),
        "assets/style.abc.css"
    );
    assert_eq!(binds.expand("no names here").unwrap(), "no names here");
    // An unterminated brace is not a name, and is left verbatim.
    assert_eq!(binds.expand("half {open").unwrap(), "half {open");
    let unbound = binds.expand("{missing}").expect_err("an unbound name");
    assert!(
        unbound.contains("css"),
        "the report lists what is bound: {unbound}"
    );
}

/// A diagnostic rendered the way the CLI renders it, for a failure report that
/// shows spans and help rather than a `Debug` dump.
struct Diagnosis<'a>(&'a dyn Diagnostic);

impl std::fmt::Display for Diagnosis<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut rendered = String::new();
        miette::GraphicalReportHandler::new()
            .with_theme(miette::GraphicalTheme::unicode_nocolor())
            .render_report(&mut rendered, self.0)?;
        f.write_str(rendered.trim())
    }
}
