//! Where a source map is written, and the comment that links an asset to it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::PathExt;

/// The link between a processed asset and the source map beside it: the map is
/// named after the file it maps, and that file names the map back.
pub(super) struct SourceMap;

impl SourceMap {
    /// Appended to the whole served name rather than replacing its extension:
    /// `app.js` maps to `app.js.map`, and `app.map` is a name the stylesheet
    /// beside it would claim too.
    const EXT: &'static str = "map";

    /// Where the map for the asset served at `dst` is written.
    pub(super) fn beside(dst: &Path) -> PathBuf {
        let mut name = OsString::from(dst.file_name().unwrap_or(dst.as_os_str()));
        name.push(".");
        name.push(Self::EXT);
        dst.with_file_name(name)
    }

    /// The media type a map is carried under when it travels inside the asset.
    const MEDIA: &'static str = "application/json;charset=utf-8;base64,";

    /// `bytes` carrying `map` itself, as a `data:` URI in place of a filename.
    pub(super) fn inlined(bytes: Vec<u8>, dst: &Path, map: &[u8]) -> Vec<u8> {
        let uri = format!("data:{}{}", Self::MEDIA, crate::digest::Base64(map));
        Self::commented(bytes, dst, &uri)
    }

    /// `bytes` with the comment naming its map appended, in the syntax the
    /// file's own language reads. The map is named by bare filename, so the
    /// reference resolves wherever the site is hosted; a file whose extension
    /// has no comment syntax is returned untouched.
    pub(super) fn linked(bytes: Vec<u8>, dst: &Path) -> Vec<u8> {
        let name = Self::beside(dst);
        let Some(name) = name.file_name().and_then(|n| n.to_str()) else {
            return bytes;
        };
        let name = name.to_owned();
        Self::commented(bytes, dst, &name)
    }

    /// `bytes` with a `sourceMappingURL` comment naming `target`, which is
    /// either the map's filename or the map itself as a `data:` URI.
    fn commented(mut bytes: Vec<u8>, dst: &Path, target: &str) -> Vec<u8> {
        let Some((open, close)) = Self::comment(dst) else {
            return bytes;
        };
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(open.as_bytes());
        bytes.extend_from_slice(b"# sourceMappingURL=");
        bytes.extend_from_slice(target.as_bytes());
        bytes.extend_from_slice(close.as_bytes());
        bytes.push(b'\n');
        bytes
    }

    /// How the language served under `dst`'s extension opens and closes a
    /// comment, or `None` for one this does not know how to annotate.
    fn comment(dst: &Path) -> Option<(&'static str, &'static str)> {
        match dst.ext().to_ascii_lowercase().as_str() {
            "js" | "mjs" | "cjs" => Some(("//", "")),
            "css" => Some(("/*", " */")),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SourceMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn a_map_is_named_after_the_whole_file_it_maps() {
        assert_eq!(
            SourceMap::beside(Path::new("css/app.a1b2.css")),
            PathBuf::from("css/app.a1b2.css.map")
        );
        assert_eq!(
            SourceMap::beside(Path::new("app.a1b2.js")),
            PathBuf::from("app.a1b2.js.map")
        );
    }

    #[test]
    fn a_script_and_a_stylesheet_each_get_their_own_comment_syntax() {
        let js = SourceMap::linked(b"const a = 1;\n".to_vec(), Path::new("app.js"));
        assert_eq!(
            String::from_utf8(js).unwrap(),
            "const a = 1;\n//# sourceMappingURL=app.js.map\n"
        );
        let css = SourceMap::linked(b"a{color:red}\n".to_vec(), Path::new("app.css"));
        assert_eq!(
            String::from_utf8(css).unwrap(),
            "a{color:red}\n/*# sourceMappingURL=app.css.map */\n"
        );
    }

    #[test]
    fn the_comment_names_the_map_without_its_directory() {
        let out = SourceMap::linked(b"x\n".to_vec(), Path::new("deep/nested/app.js"));
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("sourceMappingURL=app.js.map"), "{out}");
        assert!(!out.contains("deep/"), "{out}");
    }

    #[test]
    fn a_file_with_no_trailing_newline_gets_one_first() {
        let out = SourceMap::linked(b"const a=1".to_vec(), Path::new("app.js"));
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "const a=1\n//# sourceMappingURL=app.js.map\n"
        );
    }

    #[test]
    fn a_kind_with_no_comment_syntax_is_left_alone() {
        let bytes = b"\x89PNG".to_vec();
        assert_eq!(
            SourceMap::linked(bytes.clone(), Path::new("logo.png")),
            bytes
        );
    }
}
