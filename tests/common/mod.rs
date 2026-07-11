//! Shared harness for the integration test binaries.
// Each binary compiles this module independently and uses only part of it.
#![allow(dead_code)]

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use baudelaire::config::Config;

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
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parsed `config.kdl` with its paths rebased into the tempdir, so library
    /// calls resolve against this site instead of the test runner's cwd.
    pub fn config(&self) -> Config {
        let mut cfg = Config::parse(&self.read("config.kdl")).unwrap();
        cfg.content = self.root.join(&cfg.content);
        cfg.dist = self.root.join(&cfg.dist);
        cfg.cache.dir = self.root.join(&cfg.cache.dir);
        cfg
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

    /// Verbose build that must succeed; stdout comes back for cache-count asserts.
    pub fn build(&self) -> String {
        let out = self.run(&["build", "-v"]);
        assert!(
            out.status.success(),
            "build failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A built page under the default `public` dist.
    pub fn output(&self, rel: &str) -> String {
        self.read(&format!("public/{rel}"))
    }
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
        full.extend_from_slice(&["--port", &arg]);
        let child = site.spawn(&full);
        assert!(
            wait_for_port(port, 5000),
            "server did not start within 5000ms"
        );
        Self { _child: child, port }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn get(&self, path: &str) -> (u16, String) {
        let resp = Command::new("curl")
            .args([
                "-s",
                "-o",
                "-",
                "-w",
                "\n%{http_code}",
                &format!("http://127.0.0.1:{}{path}", self.port),
            ])
            .output()
            .expect("curl");
        let out = String::from_utf8_lossy(&resp.stdout);
        let mut lines = out.lines().rev();
        let code: u16 = lines.next().unwrap_or("0").parse().unwrap_or(0);
        let body = out[..out.len().saturating_sub(code.to_string().len() - 1)].to_string();
        (code, body)
    }
}
