//! A theme from a `.tar.gz` or a `.zip` at a URL.
//!
//! `update` fetches that same URL and takes whatever is there now, so what the
//! URL points at decides whether the copy is pinned.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::install::Lock;
use super::source::{Fetched, Fetching, Origin, Source};
use crate::error::{Result, ThemeError};
use crate::fs::Contained;
use crate::remote::{Http, Status};

/// An archive fetched over http.
pub struct Archive;

/// What an archive is still allowed to cost, counted down as it unpacks;
/// [`Archive::LIMIT`] bounds the download and says nothing about what those
/// bytes expand to, so both counters are needed.
struct Budget {
    bytes: u64,
    entries: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            bytes: Archive::UNPACKED,
            entries: Archive::ENTRIES,
        }
    }
}

impl Budget {
    /// Read one entry to its end, or fail if it would spend more than is left.
    ///
    /// The read itself is capped and never checked afterwards, at one byte past
    /// what remains, so an entry that exactly fits is told from one that ran
    /// over without a gigabyte reaching memory first.
    fn read(&mut self, url: &str, from: impl Read) -> Result<Vec<u8>> {
        self.entries = self
            .entries
            .checked_sub(1)
            .ok_or_else(|| ThemeError::crowded(url, Archive::ENTRIES))?;
        let mut bytes = Vec::new();
        from.take(self.bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|why| ThemeError::unpack(url, why))?;
        let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.bytes = self
            .bytes
            .checked_sub(read)
            .ok_or_else(|| ThemeError::unpacked(url, Archive::UNPACKED))?;
        Ok(bytes)
    }
}

/// What an archive holds: its files, and the one directory they were all
/// inside if they shared one.
pub(super) type Contents = (Option<String>, BTreeMap<PathBuf, Vec<u8>>);

impl Archive {
    /// The suffixes this source answers for; a URL without one falls through to
    /// the sources that fetch a repository.
    const SUFFIXES: [&'static str; 3] = [".tar.gz", ".tgz", ".zip"];

    /// What a theme may weigh on the wire, compressed, since this reads a
    /// remote stream into memory.
    const LIMIT: u64 = 64 * 1024 * 1024;

    /// Read only by [`Budget`].
    const UNPACKED: u64 = 256 * 1024 * 1024;
    const ENTRIES: usize = 10_000;

    /// Any lock the packed copy carried is dropped: it claims what baudelaire
    /// wrote in the project it was packed from, not in this one.
    pub(super) fn contents(url: &str) -> Result<Contents> {
        let (wrapper, files) = Self::unwrap(Self::unpack(url, Self::download(url)?)?);
        Ok((
            wrapper,
            files
                .into_iter()
                .filter(|(rel, _)| rel != Path::new(Lock::FILE))
                .collect(),
        ))
    }

    /// Fetch the bytes at `url`, reading one byte past the ceiling so
    /// [`Archive::whole`] can tell a theme that exactly fills it from one that
    /// ran over.
    fn download(url: &str) -> Result<Vec<u8>> {
        let mut body = Http::agent("fetching a theme", Status::Fatal)
            .get(url)
            .call()
            .map_err(|why| ThemeError::fetch(url, why))?
            .into_body()
            .into_reader()
            .take(Self::LIMIT + 1);
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .map_err(|why| ThemeError::fetch(url, why))?;
        Self::whole(url, bytes)
    }

    /// A prefix of an archive is not a smaller theme, so hitting
    /// [`Archive::LIMIT`] is a failure rather than a truncation that installs.
    fn whole(url: &str, bytes: Vec<u8>) -> Result<Vec<u8>> {
        if u64::try_from(bytes.len()).is_ok_and(|read| read <= Self::LIMIT) {
            Ok(bytes)
        } else {
            Err(ThemeError::oversize(url, Self::LIMIT).into())
        }
    }

    /// The files inside, by the suffix the URL was claimed by rather than by
    /// the bytes.
    fn unpack(url: &str, bytes: Vec<u8>) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        if url.to_ascii_lowercase().ends_with(".zip") {
            Self::zip(url, bytes)
        } else {
            Self::tar(url, &bytes)
        }
    }

    /// Where one entry lands, as a path that cannot leave the theme's
    /// directory: an absolute one would discard it and a `..` climb out, and
    /// the escaped path would then be locked as ours to overwrite and delete.
    ///
    /// The containment decision for every format, never one per format.
    fn inside(url: &str, entry: &Path) -> Result<PathBuf> {
        let rel = entry.strip_prefix(".").unwrap_or(entry);
        Contained::new(rel)
            .map(|rel| rel.path().to_path_buf())
            .ok_or_else(|| ThemeError::escapes(url, &entry.to_string_lossy()).into())
    }

    fn tar(url: &str, bytes: &[u8]) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut entries = BTreeMap::new();
        let mut budget = Budget::default();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        for entry in archive
            .entries()
            .map_err(|why| ThemeError::unpack(url, why))?
        {
            let mut entry = entry.map_err(|why| ThemeError::unpack(url, why))?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let named = entry
                .path()
                .map_err(|why| ThemeError::unpack(url, why))?
                .into_owned();
            let path = Self::inside(url, &named)?;
            entries.insert(path, budget.read(url, &mut entry)?);
        }
        Ok(entries)
    }

    fn zip(url: &str, bytes: Vec<u8>) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut entries = BTreeMap::new();
        let mut budget = Budget::default();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|why| ThemeError::unpack(url, why))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|why| ThemeError::unpack(url, why))?;
            if !entry.is_file() {
                continue;
            }
            let path = Self::inside(url, Path::new(entry.name()))?;
            entries.insert(path, budget.read(url, &mut entry)?);
        }
        Ok(entries)
    }

    /// Drop the wrapper directory an archive of a directory has; the test is
    /// that *everything* shares one first segment, so an archive of the theme
    /// itself is left exactly as it is.
    fn unwrap(files: BTreeMap<PathBuf, Vec<u8>>) -> (Option<String>, BTreeMap<PathBuf, Vec<u8>>) {
        let mut roots = files.keys().filter_map(|path| {
            path.components()
                .next()
                .map(|first| first.as_os_str().to_string_lossy().into_owned())
        });
        let Some(root) = roots.next() else {
            return (None, files);
        };
        if !roots.all(|other| other == root) {
            return (None, files);
        }
        let stripped = files
            .into_iter()
            .filter_map(|(path, bytes)| Some((path.strip_prefix(&root).ok()?.to_path_buf(), bytes)))
            .filter(|(path, _)| !path.as_os_str().is_empty())
            .collect();
        (Some(root), stripped)
    }

    /// The name a copy takes: the archive's wrapper directory, else the URL's
    /// filename with its suffixes removed.
    ///
    /// One ordinary directory name and nothing else, since it is joined to
    /// `themes/` to make the directory the copy is written into and deleted
    /// from.
    fn names(url: &str, wrapper: Option<String>) -> Result<String> {
        let name = wrapper.unwrap_or_else(|| {
            let file = url.rsplit('/').next().unwrap_or_default();
            Self::SUFFIXES
                .iter()
                .find_map(|suffix| file.strip_suffix(suffix))
                .unwrap_or(file)
                .to_owned()
        });
        let names = Contained::new(&name).is_some_and(|rel| rel.path().components().count() == 1);
        if names {
            Ok(name)
        } else {
            Err(ThemeError::unnamed(url).into())
        }
    }
}

impl Source for Archive {
    fn name(&self) -> &'static str {
        "archive"
    }

    /// An http URL naming an archive. The suffix is what distinguishes it from
    /// a repository URL, which is otherwise spelled the same way.
    fn parse(&self, spec: &str) -> Option<Origin> {
        let http = spec.starts_with("https://") || spec.starts_with("http://");
        let archive = Self::SUFFIXES.iter().any(|suffix| spec.ends_with(suffix));
        (http && archive).then(|| Origin::Archive {
            url: spec.to_owned(),
            subdir: None,
        })
    }

    fn owns(&self, origin: &Origin) -> bool {
        matches!(origin, Origin::Archive { .. })
    }

    fn fetch(&self, origin: &Origin, _cx: &Fetching) -> Result<Fetched> {
        let Origin::Archive { url, .. } = origin else {
            return Err(ThemeError::unsupported(origin.label()).into());
        };
        let (wrapper, files) = Self::contents(url)?;
        if files.is_empty() {
            return Err(ThemeError::empty(url).into());
        }
        Ok(Fetched {
            name: Self::names(url, wrapper)?,
            about: None,
            origin: origin.clone(),
            files,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::error::BaudelaireErrorKind;

    /// A gzipped tar of one entry, named exactly as asked.
    ///
    /// Hand-built as the 512-byte ustar block, because the `tar` program and
    /// the crate's builder both refuse a `..` name and strip a leading `/`.
    fn tarball(name: &str, body: &[u8]) -> Vec<u8> {
        fn field(header: &mut [u8; 512], at: usize, bytes: &[u8]) {
            header[at..at + bytes.len()].copy_from_slice(bytes);
        }
        let mut header = [0u8; 512];
        field(&mut header, 0, name.as_bytes());
        field(&mut header, 100, b"0000644\0");
        field(&mut header, 108, b"0000000\0");
        field(&mut header, 116, b"0000000\0");
        field(
            &mut header,
            124,
            format!("{:011o}\0", body.len()).as_bytes(),
        );
        field(&mut header, 136, b"00000000000\0");
        field(&mut header, 148, b"        ");
        field(&mut header, 156, b"0");
        field(&mut header, 257, b"ustar\0");
        field(&mut header, 263, b"00");
        let sum: u32 = header.iter().copied().map(u32::from).sum();
        field(&mut header, 148, format!("{sum:06o}\0 ").as_bytes());

        let mut out = header.to_vec();
        out.extend_from_slice(body);
        out.resize(out.len().div_ceil(512) * 512, 0);
        out.extend_from_slice(&[0u8; 1024]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&out).expect("gzip");
        gz.finish().expect("gzip")
    }

    /// A zip of one entry, named exactly as asked.
    fn zipped(name: &str, body: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(body).expect("write");
        writer.finish().expect("finish").into_inner()
    }

    fn escapes<T>(read: &Result<T>) -> bool {
        matches!(
            read,
            Err(BaudelaireErrorKind::Theme(ThemeError::Escapes { .. }))
        )
    }

    #[test]
    fn an_entry_outside_the_archive_is_refused() {
        for name in ["../../evil", "/tmp/evil", "themes/../../evil"] {
            let tar = Archive::tar("https://x.dev/t.tar.gz", &tarball(name, b"x"));
            assert!(escapes(&tar), "tar took {name}: {tar:?}");
            let zip = Archive::zip("https://x.dev/t.zip", zipped(name, b"x"));
            assert!(escapes(&zip), "zip took {name}: {zip:?}");
        }
    }

    #[test]
    fn an_ordinary_entry_is_read_by_either_format() {
        const BODY: &[u8] = b"lang \"fr\"\n";
        for name in ["theme.kdl", "./theme.kdl"] {
            let tar = Archive::tar("https://x.dev/t.tar.gz", &tarball(name, BODY)).expect("tar");
            let zip = Archive::zip("https://x.dev/t.zip", zipped(name, BODY)).expect("zip");
            for read in [tar, zip] {
                let held = read.get(Path::new("theme.kdl")).map(Vec::as_slice);
                assert_eq!(held, Some(BODY), "{name}");
            }
        }
    }

    #[test]
    fn a_copy_is_not_named_after_a_directory_above_it() {
        assert!(Archive::names("https://x.dev/d.tar.gz", Some("..".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/d.tar.gz", Some(".".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/d.tar.gz", Some("a/b".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/d.tar.gz", Some("/etc".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/...tar.gz", None).is_err());
        assert!(Archive::names("https://x.dev/.tar.gz", None).is_err());
    }

    #[test]
    fn an_archive_past_the_ceiling_is_refused_rather_than_truncated() {
        const URL: &str = "https://x.dev/t.tar.gz";
        let limit = usize::try_from(Archive::LIMIT).expect("64 MiB fits a usize");
        assert!(Archive::whole(URL, vec![0; 16]).is_ok());
        assert!(Archive::whole(URL, vec![0; limit]).is_ok());
        assert!(matches!(
            Archive::whole(URL, vec![0; limit + 1]),
            Err(BaudelaireErrorKind::Theme(ThemeError::Oversize { .. }))
        ));
    }

    fn entries(paths: &[&str]) -> BTreeMap<PathBuf, Vec<u8>> {
        paths
            .iter()
            .map(|path| (PathBuf::from(path), Vec::new()))
            .collect()
    }

    #[test]
    fn an_archive_is_claimed_by_its_suffix() {
        assert!(Archive.parse("https://x.dev/plume-1.0.0.tar.gz").is_some());
        assert!(Archive.parse("https://x.dev/plume.zip").is_some());
        assert!(Archive.parse("https://x.dev/plume.git").is_none());
        assert!(Archive.parse("./plume.zip").is_none());
    }

    #[test]
    fn one_wrapper_directory_is_dropped() {
        let (wrapper, files) = Archive::unwrap(entries(&[
            "plume-1.0.0/templates/page.typ",
            "plume-1.0.0/theme.kdl",
        ]));
        assert_eq!(wrapper.as_deref(), Some("plume-1.0.0"));
        assert_eq!(
            files.keys().map(PathBuf::as_path).collect::<Vec<&Path>>(),
            [Path::new("templates/page.typ"), Path::new("theme.kdl")]
        );
    }

    #[test]
    fn an_archive_of_the_theme_itself_keeps_its_shape() {
        let (wrapper, files) = Archive::unwrap(entries(&["templates/page.typ", "theme.kdl"]));
        assert!(wrapper.is_none());
        assert_eq!(files.len(), 2);
        assert!(files.contains_key(Path::new("templates/page.typ")));
    }

    #[test]
    fn a_copy_is_named_after_the_wrapper_or_the_url() {
        assert_eq!(
            Archive::names("https://x.dev/d.tar.gz", Some("plume-1.0.0".to_owned()))
                .expect("named"),
            "plume-1.0.0"
        );
        assert_eq!(
            Archive::names("https://x.dev/plume.tar.gz", None).expect("named"),
            "plume"
        );
    }

    /// Exercised on [`Budget`] itself rather than through a real bomb, which
    /// would read into memory the very thing the guard exists to refuse.
    #[test]
    fn an_entry_may_not_unpack_past_what_is_left() {
        let mut budget = Budget {
            bytes: 4,
            entries: 8,
        };
        assert_eq!(
            budget
                .read("https://x.dev/t.zip", &b"abcd"[..])
                .expect("fits"),
            b"abcd"
        );
        assert!(matches!(
            budget.read("https://x.dev/t.zip", &b"e"[..]),
            Err(BaudelaireErrorKind::Theme(ThemeError::Unpacked { .. }))
        ));
    }

    #[test]
    fn an_archive_may_not_hold_more_entries_than_a_theme_has() {
        let mut budget = Budget {
            bytes: 1024,
            entries: 1,
        };
        budget.read("https://x.dev/t.zip", &b""[..]).expect("first");
        assert!(matches!(
            budget.read("https://x.dev/t.zip", &b""[..]),
            Err(BaudelaireErrorKind::Theme(ThemeError::Crowded { .. }))
        ));
    }
}
