//! Shared harness for the integration test binaries.
// Each binary compiles this module independently and uses only part of it.
#![allow(dead_code)]

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use baudelaire::BaudelaireErrorKind;
use baudelaire::config::Config;
use baudelaire::engine::{Engine, Mode, Stats};
use baudelaire::ui::{Level, Ui};
use baudelaire::world::Project;

/// The minimal config nearly every test starts from: content in, `public` out,
/// clean URLs on. Tests needing more compose their own.
pub const CONFIG: &str = "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n";

/// A throwaway site rooted in a tempdir, removed on drop.
pub struct Site {
    _tmp: tempfile::TempDir,
    pub root: PathBuf,
}

impl Site {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        Self { _tmp: tmp, root }
    }

    /// `new` plus a `config.kdl`, the first thing nearly every test writes.
    pub fn with(config: &str) -> Self {
        let site = Self::new();
        site.write("config.kdl", config);
        site
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }

    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap()
    }

    pub fn write(&self, rel: &str, contents: &str) {
        self.write_bytes(rel, contents.as_bytes());
    }

    pub fn write_bytes(&self, rel: &str, contents: &[u8]) {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// Names of the immediate files in `rel`, for asserting on generated
    /// (possibly fingerprinted) filenames.
    pub fn files(&self, rel: &str) -> Vec<String> {
        fs::read_dir(self.path(rel))
            .map(|rd| {
                rd.filter_map(std::result::Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parsed `config.kdl` with its paths rebased into the tempdir, so library
    /// calls resolve against this site instead of the test runner's cwd.
    pub fn config(&self) -> Config {
        self.try_config().expect("config")
    }

    /// [`Site::config`] as a `Result`, for callers that must report a malformed
    /// config rather than panic on it (the scenario runner, whose whole job is
    /// to survive one broken case and report the rest).
    pub fn try_config(&self) -> baudelaire::Result<Config> {
        let mut cfg = Config::load(&self.read("config.kdl"), &self.root)?;
        cfg.root.clone_from(&self.root);
        cfg.paths.content = self.root.join(&cfg.paths.content);
        cfg.paths.dist = self.root.join(&cfg.paths.dist);
        cfg.paths.assets = self.root.join(&cfg.paths.assets);
        cfg.paths.templates = self.root.join(&cfg.paths.templates);
        cfg.paths.r#static = self.root.join(&cfg.paths.r#static);
        cfg.cache.dir = self.root.join(&cfg.cache.dir);
        Ok(cfg)
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_baudelaire"));
        cmd.args(args).current_dir(&self.root);
        cmd
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().expect("run binary")
    }

    /// Spawn a long-running command (e.g. `serve`), reaped on drop.
    pub fn spawn(&self, args: &[&str]) -> Child {
        Child(self.cmd(args).spawn().expect("spawn binary"))
    }

    /// Verbose build through the real binary, which must succeed; its logs come
    /// back for tests that assert on the CLI's own output.
    ///
    /// Prefer [`Site::stats`] for anything else. The two are not interchangeable
    /// within one test: the binary resolves paths relative to its cwd while
    /// `config()` rebases them absolute, and `Config::hash` covers the path
    /// fields, so the two never share a cache.
    pub fn build(&self) -> String {
        let out = self.run(&["build", "-v"]);
        assert!(
            out.status.success(),
            "build failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Human-facing logs (per-page lines, summary) live on stderr.
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// Build in-process and return the engine's own [`Stats`].
    ///
    /// Preferred over scraping `build()`'s summary text for cache counts: the
    /// numbers come from the engine rather than from prose that could be
    /// reworded, `!contains("1 cached")` cannot pass by accident, and the
    /// coverage tool sees this path (it instruments the test binary, not a
    /// spawned subprocess).
    pub fn stats(&self) -> Stats {
        self.stats_with(|_| {})
    }

    /// [`Site::stats`] with the config adjusted first, standing in for the CLI
    /// overrides a subprocess run would pass (`--drafts`, `--base-url`).
    pub fn stats_with(&self, tweak: impl FnOnce(&mut Config)) -> Stats {
        self.try_stats(tweak).expect("build")
    }

    /// The in-process build's own `Result`, for tests asserting on a failure:
    /// the typed diagnostic and its code, rather than the words the CLI printed.
    pub fn build_error(&self) -> BaudelaireErrorKind {
        match self.try_stats(|_| {}) {
            Err(err) => err,
            Ok(stats) => panic!("build should have failed, but produced {stats:?}"),
        }
    }

    /// The plain in-process build as a `Result`: neither success nor failure is
    /// assumed, so a caller can decide which one the case wanted.
    pub fn try_build(&self) -> baudelaire::Result<Stats> {
        self.try_stats(|_| {})
    }

    fn try_stats(&self, tweak: impl FnOnce(&mut Config)) -> baudelaire::Result<Stats> {
        let mut config = self.try_config()?;
        tweak(&mut config);
        Engine::new(config, Mode::Build)?.build(&Ui::new(Level::Silent))
    }

    /// Run `check` in-process and return its own `Result`, for tests asserting
    /// on the validation passes rather than on output files.
    pub fn try_check(&self, tweak: impl FnOnce(&mut Config)) -> baudelaire::Result<Stats> {
        let mut config = self.config();
        tweak(&mut config);
        Engine::new(config, Mode::Check)?.check(&Ui::new(Level::Silent))
    }

    /// A built page under the default `public` dist.
    pub fn output(&self, rel: &str) -> String {
        self.read(&format!("public/{rel}"))
    }
}

/// A `Ui` that prints nothing, for tests that drive the library in-process.
pub fn silent() -> Ui {
    Ui::new(Level::Silent)
}

/// A [`Project`] for a test config: module evaluation needs the real world.
pub fn project(cfg: &Config) -> Project {
    Project::new(cfg, baudelaire::world::Mode::Build).expect("project")
}

/// A spawned child process, killed and reaped on drop so an early panic never
/// leaks a child holding its port.
pub struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Poll until `port` accepts a connection, up to `timeout_ms`.
pub fn wait_for_port(port: u16, timeout_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A `serve` process on its own free port, reachable before `start` returns
/// and killed on drop.
pub struct Serve {
    _child: Child,
    port: u16,
}

impl Serve {
    pub fn start(site: &Site, args: &[&str]) -> Self {
        let port = free_port();
        let arg = port.to_string();
        let mut full = vec!["serve"];
        full.extend_from_slice(args);
        // `serve.open` defaults to on, so a test whose config forgets
        // `serve { open #false }` launches a real browser on the runner. Forced
        // here rather than per test, where it can be forgotten again.
        full.extend_from_slice(&["--port", &arg, "--no-open"]);
        let child = site.spawn(&full);
        assert!(
            wait_for_port(port, 5000),
            "server did not start within 5000ms"
        );
        Self {
            _child: child,
            port,
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn get(&self, path: &str) -> (u16, String) {
        self.request(path, false)
    }

    /// Like [`Serve::get`], but sends the path verbatim (`curl --path-as-is`) so
    /// `..` segments reach the server instead of being collapsed by the client:
    /// the only way to exercise path-traversal defenses.
    pub fn get_raw(&self, path: &str) -> (u16, String) {
        self.request(path, true)
    }

    fn request(&self, path: &str, raw: bool) -> (u16, String) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut cmd = Command::new("curl");
        cmd.args(["-s", "-o", "-", "-w", "\n%{http_code}"]);
        if raw {
            cmd.arg("--path-as-is");
        }
        let resp = cmd.arg(&url).output().expect("curl");
        // `-w "\n%{http_code}"` appends a newline and the status to the body, so
        // split at the last newline rather than counting characters: the old
        // arithmetic was off by three and left `"\n2"` on every body, so every
        // negative assertion was checking slightly wrong data.
        let out = String::from_utf8_lossy(&resp.stdout);
        let (body, status) = out.rsplit_once('\n').unwrap_or_else(|| ("", out.as_ref()));
        (status.trim().parse().unwrap_or(0), body.to_owned())
    }
}

/// Whether `name`'s extension is `ext`, spelled without the dot.
///
/// Generated names are fingerprinted (`style.<hash>.css`), so the assertion is
/// about the extension itself rather than a `.css` suffix on the whole string.
pub fn has_ext(name: &str, ext: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|found| found == ext)
}

/// The fixture site both deploy e2e suites reconcile against: three files under
/// `public/`, one of them nested, so a run exercises directory creation as well
/// as plain uploads.
pub fn dist(site: &Site) {
    site.write("public/index.html", "<h1>home</h1>");
    site.write("public/posts/a.html", "post a");
    site.write("public/style.css", "body{}");
}
