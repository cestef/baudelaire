//! A theme from an archive over http.
//!
//! The source with no tooling behind it: a `.tar.gz` or a `.zip` at a URL, which
//! is what every forge offers for a tag without anyone cloning anything, and
//! what a theme published as a release asset is.
//!
//! What it cannot do is follow anything. An archive is a snapshot at a URL, so
//! `update` fetches that same URL and takes whatever is there now: if the URL
//! names a tag, the copy is pinned; if it names a branch's tip, it moves. That
//! is the URL's business, and the record says exactly which one was used.

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

/// What an archive holds: its files, and the one directory they were all
/// inside if they shared one.
pub(super) type Contents = (Option<String>, BTreeMap<PathBuf, Vec<u8>>);

impl Archive {
    /// The suffixes this source answers for. A URL without one is not an
    /// archive as far as this is concerned, and falls through to the sources
    /// that fetch a repository.
    const SUFFIXES: [&'static str; 3] = [".tar.gz", ".tgz", ".zip"];

    /// What a theme may weigh, uncompressed and in total.
    ///
    /// A ceiling, not a guess: this reads a remote stream into memory, and
    /// without a limit a hostile (or merely wrong) URL decides how much of it
    /// to use. The shipped themes are ~60 KiB each, so this is three orders of
    /// magnitude of room.
    const LIMIT: u64 = 64 * 1024 * 1024;

    /// The files an archive at `url` holds, and the wrapper directory they
    /// were inside, if they shared one.
    ///
    /// The whole of what this source does, so the forge source can spell a URL
    /// and reuse everything after it: download, unpack, drop the wrapper, and
    /// leave behind whatever record the packed copy carried.
    pub(super) fn contents(url: &str) -> Result<Contents> {
        let (wrapper, files) = Self::unwrap(Self::unpack(url, Self::download(url)?)?);
        // A lock inside an archive is whatever project it was packed from
        // saying what baudelaire wrote *there*, which is no claim on this copy.
        Ok((
            wrapper,
            files
                .into_iter()
                .filter(|(rel, _)| rel != Path::new(Lock::FILE))
                .collect(),
        ))
    }

    /// Fetch the bytes at `url`.
    fn download(url: &str) -> Result<Vec<u8>> {
        let mut body = Http::agent("fetching a theme", Status::Fatal)
            .get(url)
            .call()
            .map_err(|why| ThemeError::fetch(url, why))?
            .into_body()
            .into_reader()
            .take(Self::LIMIT);
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .map_err(|why| ThemeError::fetch(url, why))?;
        Ok(bytes)
    }

    /// The files inside, by the archive's own kind.
    fn unpack(url: &str, bytes: Vec<u8>) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        // The suffix, not the bytes: an archive is claimed by its spelling in
        // the first place, so it is read by the same test it was claimed by.
        match url.to_ascii_lowercase().ends_with(".zip") {
            true => Self::zip(url, bytes),
            false => Self::tar(url, &bytes),
        }
    }

    /// Where one entry lands, as a path that cannot leave the directory the
    /// theme is unpacked into.
    ///
    /// THE containment decision, and deliberately not one per format. Every key
    /// of a [`Fetched`]'s files is later joined to the theme's directory, where
    /// `Path::join` on an absolute entry silently discards that directory and a
    /// `..` climbs out of it; the escaped path is then recorded in the lock,
    /// reads back as pristine, and so is overwritten by `theme update` and
    /// deleted by `theme remove`. Deciding it per format is exactly how the zip
    /// branch came to have a guard and the tar branch none.
    ///
    /// A leading `./` is dropped first: `tar -czf x.tgz .` spells every entry
    /// that way, and it is the same relative path written differently rather
    /// than a step out of anything.
    fn inside(url: &str, entry: &Path) -> Result<PathBuf> {
        let rel = entry.strip_prefix(".").unwrap_or(entry);
        Contained::new(rel)
            .map(|rel| rel.path().to_path_buf())
            .ok_or_else(|| ThemeError::escapes(url, &entry.to_string_lossy()).into())
    }

    fn tar(url: &str, bytes: &[u8]) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut entries = BTreeMap::new();
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
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|why| ThemeError::unpack(url, why))?;
            entries.insert(path, bytes);
        }
        Ok(entries)
    }

    fn zip(url: &str, bytes: Vec<u8>) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut entries = BTreeMap::new();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|why| ThemeError::unpack(url, why))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|why| ThemeError::unpack(url, why))?;
            if !entry.is_file() {
                continue;
            }
            // The name as the archive wrote it, judged by the same guard the
            // tar branch uses: `enclosed_name` is the zip reader's own reading
            // of the question, and two readings of one question is what left
            // the other branch without an answer at all.
            let path = Self::inside(url, Path::new(entry.name()))?;
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|why| ThemeError::unpack(url, why))?;
            entries.insert(path, bytes);
        }
        Ok(entries)
    }

    /// Drop the wrapper directory an archive of a directory has.
    ///
    /// Every forge's tarball is one top-level `name-ref/`, and unpacking it as
    /// the theme would make every layout `plume-1.0.0/templates/page.typ`. The
    /// test is that *everything* shares one first segment: an archive of the
    /// theme itself, with `templates/` at the root, has more than one and is
    /// left exactly as it is.
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

    /// The name a copy takes, from the archive's own wrapper directory when it
    /// had one, else from the URL's filename with its suffixes removed.
    ///
    /// One ordinary directory name and nothing else, because the name is joined
    /// to `themes/` to make the directory the copy is written into and deleted
    /// from: `.` would name that directory itself and `..` the project root. A
    /// URL is enough to produce either (`.../...tar.gz` leaves `..`), and so was
    /// an archive whose entries all began `../`, whose shared first component
    /// `unwrap` read as the wrapper directory.
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
        match names {
            true => Ok(name),
            false => Err(ThemeError::unnamed(url).into()),
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
    /// Hand-built, because nothing that writes a tar willingly writes this one:
    /// the `tar` program refuses a `..` name and strips a leading `/`, and the
    /// crate's builder does the same. The header is the 512-byte ustar block,
    /// whose fields are at fixed offsets and whose checksum is taken with its
    /// own field read as eight spaces.
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
        // Everything is a whole block, and two empty ones end the archive.
        out.resize(out.len().div_ceil(512) * 512, 0);
        out.extend_from_slice(&[0u8; 1024]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&out).expect("gzip");
        gz.finish().expect("gzip")
    }

    /// A zip of one entry, named exactly as asked: the writer records the name
    /// it is given, which is all this fixture needs.
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

    /// An entry naming a file outside the archive is refused before a byte is
    /// written, in both formats.
    ///
    /// The tar branch had no guard at all: it inserted `entry.path()` as it
    /// came, and the install joined that to the theme directory, where an
    /// absolute entry discards the directory outright and a `..` climbs above
    /// it. The lock then claimed whatever it landed on, so `update` overwrote
    /// and `remove` deleted a file outside the project.
    #[test]
    fn an_entry_outside_the_archive_is_refused() {
        for name in ["../../evil", "/tmp/evil", "themes/../../evil"] {
            let tar = Archive::tar("https://x.dev/t.tar.gz", &tarball(name, b"x"));
            assert!(escapes(&tar), "tar took {name}: {tar:?}");
            let zip = Archive::zip("https://x.dev/t.zip", zipped(name, b"x"));
            assert!(escapes(&zip), "zip took {name}: {zip:?}");
        }
    }

    /// ...and an ordinary entry still comes through, including the `./name`
    /// spelling `tar -czf x.tgz .` writes, which is the same relative path and
    /// not a step out of anything.
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

    /// The name a copy is known by is joined to `themes/`, so anything but one
    /// ordinary directory name would write and later delete somewhere else:
    /// `..` is the project root itself, and both an archive's wrapper directory
    /// and a URL's filename can spell it.
    #[test]
    fn a_copy_is_not_named_after_a_directory_above_it() {
        assert!(Archive::names("https://x.dev/d.tar.gz", Some("..".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/d.tar.gz", Some(".".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/d.tar.gz", Some("a/b".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/d.tar.gz", Some("/etc".to_owned())).is_err());
        assert!(Archive::names("https://x.dev/...tar.gz", None).is_err());
        assert!(Archive::names("https://x.dev/.tar.gz", None).is_err());
    }

    fn entries(paths: &[&str]) -> BTreeMap<PathBuf, Vec<u8>> {
        paths
            .iter()
            .map(|path| (PathBuf::from(path), Vec::new()))
            .collect()
    }

    /// A URL is an archive by its suffix, because a repository URL is spelled
    /// the same way and only the suffix tells them apart.
    #[test]
    fn an_archive_is_claimed_by_its_suffix() {
        assert!(Archive.parse("https://x.dev/plume-1.0.0.tar.gz").is_some());
        assert!(Archive.parse("https://x.dev/plume.zip").is_some());
        assert!(Archive.parse("https://x.dev/plume.git").is_none());
        assert!(Archive.parse("./plume.zip").is_none());
    }

    /// A forge's tarball wraps the theme in one directory, and unpacking that
    /// as the theme would put every layout one level down.
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

    /// ...and an archive of the theme itself is left alone: two first segments
    /// mean no wrapper, and stripping one would eat a real directory.
    #[test]
    fn an_archive_of_the_theme_itself_keeps_its_shape() {
        let (wrapper, files) = Archive::unwrap(entries(&["templates/page.typ", "theme.kdl"]));
        assert!(wrapper.is_none());
        assert_eq!(files.len(), 2);
        assert!(files.contains_key(Path::new("templates/page.typ")));
    }

    /// The name: the wrapper's, else the URL's filename without the suffix that
    /// made it an archive.
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
}
