//! Typed accessors over [`KdlNode`] and [`KdlEntry`]: the primitives every
//! config rule is written in terms of. The [`KdlValue`] half lives in
//! [`super::value`].

use std::time::Duration;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use crate::config::assets::targets::Version;
use crate::config::dispatch::Arity;
use crate::config::lint::severity::{Level, Severity};
use crate::config::url::BaseUrl;
use crate::config::value::{Kdl, ValueExt};
use crate::error::{ConfigError, Result};
use crate::ui::{Bytes, Dur};

/// The loopback interface, by the spellings a URL authority can name it.
struct Loopback;

impl Loopback {
    /// Whether a URL's post-scheme remainder names loopback.
    ///
    /// Userinfo is stripped at the *last* `@` first: the authority of
    /// `http://localhost:9000@evil.com` is `evil.com`, not loopback.
    fn at(rest: &str) -> bool {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let host = host.rsplit_once(':').map_or(host, |(head, port)| {
            if port.chars().all(|c| c.is_ascii_digit()) {
                head
            } else {
                host
            }
        });
        matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
    }
}

/// Typed reads of a [`KdlNode`]'s arguments and children, each erroring with a
/// span into the config text rather than coercing.
///
/// # What a list key written with no values means
///
/// A list that **replaces** what the key holds ([`words`](NodeExt::words),
/// [`bounds`](NodeExt::bounds), [`mapped`](NodeExt::mapped)) reads a bare node
/// as the *empty list*, which is the one spelling a profile has for clearing an
/// inherited list. A list that **amends** defaults in the `-name` grammar
/// ([`toggled`](NodeExt::toggled), [`features`](NodeExt::features)) refuses a
/// bare node, since it would amend nothing.
pub(super) trait NodeExt {
    fn span(&self) -> SourceSpan;
    fn string(&self, text: &str, idx: usize) -> Result<String>;
    fn arg(&self, text: &str, idx: usize) -> Result<&KdlValue>;
    fn boolean(&self, text: &str, idx: usize) -> Result<bool>;
    fn int(&self, text: &str, idx: usize) -> Result<i64>;
    fn count(&self, text: &str, idx: usize) -> Result<usize>;
    fn port(&self, text: &str, idx: usize) -> Result<u16>;
    /// A byte size, written either as a plain integer of bytes (`js 0`) or as a
    /// string carrying a unit (`html "50kB"`).
    fn size(&self, text: &str, idx: usize) -> Result<Bytes>;
    /// A length of time, written either as a plain integer of seconds
    /// (`timeout 30`) or as a string carrying a unit (`fresh "7d"`).
    fn duration(&self, text: &str, idx: usize) -> Result<Duration>;
    /// A browser version, `major[.minor[.patch]]`, read by [`Version::parse`].
    fn version(&self, text: &str, idx: usize) -> Result<Version>;
    /// A lint rule's loudness, written either as a boolean (`alt #false`) or as
    /// a severity naming itself (`alt "warn"`).
    fn level(&self, text: &str, idx: usize) -> Result<Level>;
    fn url(&self, text: &str, idx: usize) -> Result<String>;
    /// The path a generated asset is served from, relative to the asset root
    /// and staying inside it.
    fn asset(&self, text: &str, idx: usize) -> Result<std::path::PathBuf>;
    fn base_url(&self, text: &str, idx: usize) -> Result<String>;
    /// A permalink template, or a piece of one, checked by [`Permalink::parse`]:
    /// an unknown placeholder, an unterminated `{`, and any `..` segment are
    /// errors at the span the author wrote.
    ///
    /// The keys that are *parts* of an index's permalink (`paginate { mount }`,
    /// `prefix`) are held to the same rule, so a `..` there cannot publish a
    /// page at a URL nobody asked for.
    ///
    /// [`Permalink::parse`]: crate::config::Permalink::parse
    fn template(&self, text: &str, idx: usize) -> Result<String>;
    fn block(&self, text: &str) -> Result<&KdlDocument>;
    /// The node's `{ .. }` children parsed as `(id, item)` pairs, erroring on a
    /// duplicate id rather than silently losing one side.
    fn unique<T>(
        &self,
        text: &str,
        noun: &'static str,
        each: fn(&KdlNode, &str) -> Result<(String, T)>,
    ) -> Result<Vec<(String, T)>>;
    /// The node's `{ .. }` children as a `name -> scalar` table, for the
    /// free-form dictionaries (`client { .. }`, a language's `strings { .. }`).
    fn table(&self, text: &str) -> Result<Vec<(String, crate::codegen::Value)>>;
    /// The node's `{ .. }` children as a `name -> string` table
    /// (`typst { inputs { .. } }`).
    fn pairs(&self, text: &str) -> Result<Vec<(String, String)>>;
    fn features(&self, text: &str) -> Result<Vec<String>>;
    fn words(&self, text: &str) -> Result<Vec<String>>;
    /// The node's positional integer args, each range-checked by
    /// [`ValueExt::bounded`] at the span of the value: `widths 480 960 1440`.
    fn bounds<T>(&self, text: &str, min: T, max: T) -> Result<Vec<T>>
    where
        T: TryFrom<i64> + Into<i64> + Copy;
    fn mapped<T: super::Named>(&self, text: &str) -> Result<Vec<T>>;
    /// A list of names over a fixed set, where `-name` removes one from
    /// `defaults` and a bare name adds one: `extensions "math" "-tables"`.
    ///
    /// The shape a setting takes when it has defaults worth keeping, so naming
    /// one extra does not drop every default the line did not repeat.
    fn toggled<T: super::Named>(&self, text: &str, defaults: &[T]) -> Result<Vec<T>>;
    /// This node's first argument as a file name that stays inside the output
    /// directory, judged by [`crate::fs::Contained`].
    fn contained(&self, text: &str) -> Result<String>;
}

/// One name in a toggled list, and whether it is being added or removed: the
/// `-name` / `+name` / `name` grammar, in the one place that knows it.
struct Toggle<'a> {
    name: &'a str,
    /// `false` for `-name`.
    add: bool,
}

impl<'a> Toggle<'a> {
    fn of(raw: &'a str) -> Self {
        raw.strip_prefix('-').map_or_else(
            || Self {
                name: raw.strip_prefix('+').unwrap_or(raw),
                add: true,
            },
            |name| Self { name, add: false },
        )
    }

    /// A list in this grammar amends the key's defaults, so one naming nothing
    /// is refused rather than parsed as a line that configures nothing.
    fn required(node: &KdlNode, text: &str, named: usize) -> Result<()> {
        if named > 0 {
            return Ok(());
        }
        Err(ConfigError::missing_arg(text, node.name().value(), NodeExt::span(node)).into())
    }
}

impl NodeExt for KdlNode {
    /// Bridge kdl's `miette::SourceSpan` (its own miette 7) to ours.
    // The spelled-out type shows this is kdl's inherent method, not a recursion.
    #[allow(clippy::use_self)]
    fn span(&self) -> SourceSpan {
        let s = KdlNode::span(self);
        SourceSpan::new(s.offset().into(), s.len())
    }

    fn string(&self, text: &str, idx: usize) -> Result<String> {
        self.arg(text, idx)?.as_str(text, NodeExt::span(self))
    }

    fn arg(&self, text: &str, idx: usize) -> Result<&KdlValue> {
        self.get(idx).map_or_else(
            || Err(ConfigError::missing_arg(text, self.name().value(), NodeExt::span(self)).into()),
            Ok,
        )
    }

    /// A flag node's boolean: a bare node (`prune`) enables, a present argument
    /// must be a KDL boolean (`prune #false`); anything else is a type error,
    /// never a silent coercion.
    fn boolean(&self, text: &str, idx: usize) -> Result<bool> {
        self.get(idx).map_or_else(
            || Ok(true),
            |value| value.boolean(text, NodeExt::span(self)),
        )
    }

    fn int(&self, text: &str, idx: usize) -> Result<i64> {
        self.arg(text, idx)?.integer(text, NodeExt::span(self))
    }

    /// A non-negative integer argument (a count), erroring on negatives with
    /// the node's own name in the message.
    fn count(&self, text: &str, idx: usize) -> Result<usize> {
        let n = self.int(text, idx)?;
        usize::try_from(n).map_err(|_| {
            ConfigError::negative_count(text, self.name().value(), n, NodeExt::span(self)).into()
        })
    }

    /// A TCP port argument, range-checked so `port 99999` errors instead of
    /// wrapping to a different port.
    fn port(&self, text: &str, idx: usize) -> Result<u16> {
        let n = self.int(text, idx)?;
        u16::try_from(n).map_err(|_| ConfigError::port_range(text, n, NodeExt::span(self)).into())
    }

    /// An integer is bytes, and a string is read by [`Bytes::parse`].
    fn size(&self, text: &str, idx: usize) -> Result<Bytes> {
        let span = NodeExt::span(self);
        let value = self.arg(text, idx)?;
        if let Some(written) = value.as_string() {
            return Bytes::parse(written)
                .ok_or_else(|| ConfigError::bad_size(text, written, span).into());
        }
        Ok(Bytes(self.count(text, idx)? as u64))
    }

    /// An integer is seconds, and a string is read by [`Dur::parse`].
    fn duration(&self, text: &str, idx: usize) -> Result<Duration> {
        let span = NodeExt::span(self);
        let value = self.arg(text, idx)?;
        if let Some(written) = value.as_string() {
            return Dur::parse(written)
                .map(|d| d.0)
                .ok_or_else(|| ConfigError::bad_duration(text, written, span).into());
        }
        Ok(Duration::from_secs(self.count(text, idx)? as u64))
    }

    /// A lint rule's loudness. A boolean is "on at the site's default" or
    /// "off"; a string names the severity and so overrides `strict`.
    ///
    /// Everything that is not a string goes through [`NodeExt::boolean`], which
    /// keeps the bare `lint { alt }` spelling working.
    fn level(&self, text: &str, idx: usize) -> Result<Level> {
        let span = NodeExt::span(self);
        match self.get(idx) {
            Some(value) if value.as_string().is_some() => {
                value.one::<Severity>(text, span).map(Level::named)
            }
            _ => self.boolean(text, idx).map(Level::flag),
        }
    }

    /// A browser version. Always a string: `safari 15.4` is a *float* in KDL,
    /// and `15.10` would round-trip as `15.1`.
    ///
    /// A value that is not a string is reported as a bad *version* rather than
    /// as a type mismatch, so the help explaining the quoting reaches the author
    /// who wrote it unquoted.
    fn version(&self, text: &str, idx: usize) -> Result<Version> {
        let span = NodeExt::span(self);
        let value = self.arg(text, idx)?;
        let written = match value.as_string() {
            Some(written) => written.to_owned(),
            None => {
                return Err(ConfigError::bad_version(text, &Kdl(value).to_string(), span).into());
            }
        };
        Version::parse(&written)
            .ok_or_else(|| ConfigError::bad_version(text, &written, span).into())
    }

    /// A generated asset's served path: relative to the asset root, and inside
    /// it.
    ///
    /// Checked here rather than where the file is written, because by then every
    /// page carrying the asset has already linked the same string.
    fn asset(&self, text: &str, idx: usize) -> Result<std::path::PathBuf> {
        let value = self.string(text, idx)?;
        let path = std::path::PathBuf::from(&value);
        let escapes = path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || path.file_name().is_none();
        if escapes {
            Err(ConfigError::not_an_asset_path(text, &value, NodeExt::span(self)).into())
        } else {
            Ok(path)
        }
    }

    /// A base-URL argument, required to be `https://` unless it names the
    /// loopback interface.
    ///
    /// Credentials travel to these hosts and nothing downstream re-checks the
    /// scheme. Loopback stays allowed so a local MinIO or PDS still works.
    fn url(&self, text: &str, idx: usize) -> Result<String> {
        let value = self.string(text, idx)?;
        let bad = || ConfigError::insecure_url(text, &value, NodeExt::span(self)).into();
        let Some((scheme, rest)) = value.split_once("://") else {
            return Err(bad());
        };
        match scheme.to_ascii_lowercase().as_str() {
            "https" => Ok(value),
            "http" if Loopback::at(rest) => Ok(value),
            _ => Err(bad()),
        }
    }

    /// The site's own base URL: a scheme and a host, any scheme.
    ///
    /// Separate from [`url`](NodeExt::url), which guards a host credentials are
    /// sent to and so demands https.
    fn base_url(&self, text: &str, idx: usize) -> Result<String> {
        let value = self.string(text, idx)?;
        if BaseUrl::absolute(&value) {
            Ok(value)
        } else {
            Err(ConfigError::not_absolute_url(text, &value, NodeExt::span(self)).into())
        }
    }

    fn template(&self, text: &str, idx: usize) -> Result<String> {
        let raw = self.string(text, idx)?;
        if let Err(why) = crate::config::permalink::Permalink::parse(&raw) {
            return Err(ConfigError::at(text, why.into(), NodeExt::span(self)).into());
        }
        Ok(raw)
    }

    fn block(&self, text: &str) -> Result<&KdlDocument> {
        self.children()
            .ok_or_else(|| ConfigError::missing_children(text, NodeExt::span(self)).into())
    }

    fn unique<T>(
        &self,
        text: &str,
        noun: &'static str,
        each: fn(&KdlNode, &str) -> Result<(String, T)>,
    ) -> Result<Vec<(String, T)>> {
        let children = self.block(text)?;
        let mut out: Vec<(String, T)> = Vec::new();
        for node in children.nodes() {
            let (id, item) = each(node, text)?;
            if out.iter().any(|(seen, _)| *seen == id) {
                return Err(ConfigError::duplicate_id(text, noun, &id, NodeExt::span(node)).into());
            }
            out.push((id, item));
        }
        Ok(out)
    }

    fn table(&self, text: &str) -> Result<Vec<(String, crate::codegen::Value)>> {
        self.block(text)?
            .nodes()
            .iter()
            .map(|child| {
                Arity::Every.check(child, text)?;
                let span = NodeExt::span(child);
                let mut values = Vec::new();
                let mut index = 0;
                while let Some(arg) = child.get(index) {
                    values.push(arg.scalar(text, span)?);
                    index += 1;
                }
                let value = match values.len() {
                    0 => child.arg(text, 0)?.scalar(text, span)?,
                    1 => values.remove(0),
                    _ => crate::codegen::Value::Array(values),
                };
                Ok((child.name().value().to_owned(), value))
            })
            .collect()
    }

    fn pairs(&self, text: &str) -> Result<Vec<(String, String)>> {
        self.block(text)?
            .nodes()
            .iter()
            .map(|child| {
                Arity::Args(1).check(child, text)?;
                Ok((child.name().value().to_owned(), child.string(text, 0)?))
            })
            .collect()
    }

    fn features(&self, text: &str) -> Result<Vec<String>> {
        let positional: Vec<_> = self
            .entries()
            .iter()
            .filter(|e| e.name().is_none())
            .collect();
        Toggle::required(self, text, positional.len())?;
        positional
            .iter()
            .map(|entry| {
                let span = EntryExt::span(*entry);
                let raw = entry.value().as_str(text, span)?;
                let toggle = Toggle::of(&raw);
                if !toggle.add && toggle.name == "html" {
                    return Err(ConfigError::feature_removal(text, toggle.name, span).into());
                }
                Ok(if toggle.add {
                    toggle.name.to_owned()
                } else {
                    format!("-{}", toggle.name)
                })
            })
            .collect()
    }

    /// The node's positional string args, verbatim (e.g. `stopwords "a" "the"`).
    fn words(&self, text: &str) -> Result<Vec<String>> {
        let span = NodeExt::span(self);
        self.entries()
            .iter()
            .filter(|e| e.name().is_none())
            .map(|e| e.value().as_str(text, span))
            .collect()
    }

    fn bounds<T>(&self, text: &str, min: T, max: T) -> Result<Vec<T>>
    where
        T: TryFrom<i64> + Into<i64> + Copy,
    {
        self.entries()
            .iter()
            .filter(|e| e.name().is_none())
            .map(|e| e.value().bounded::<T>(text, EntryExt::span(e), min, max))
            .collect()
    }

    /// The node's positional string args mapped through `T`'s name table,
    /// erroring on the first unknown name with a nearest-match hint.
    fn mapped<T: super::Named>(&self, text: &str) -> Result<Vec<T>> {
        let mut out: Vec<T> = Vec::new();
        for entry in self.entries().iter().filter(|e| e.name().is_none()) {
            let span = EntryExt::span(entry);
            let value: T = entry.value().one(text, span)?;
            if out.contains(&value) {
                let name = entry.value().as_str(text, span)?;
                return Err(
                    ConfigError::duplicate_entry(text, &name, self.name().value(), span).into(),
                );
            }
            out.push(value);
        }
        Ok(out)
    }

    fn toggled<T: super::Named>(&self, text: &str, defaults: &[T]) -> Result<Vec<T>> {
        let mut out = defaults.to_vec();
        let mut seen: Vec<T> = Vec::new();
        for entry in self.entries() {
            let span = EntryExt::span(entry);
            if let Some(key) = entry.name() {
                return Err(ConfigError::unexpected_argument(
                    text,
                    &format!("{}={}", key.value(), Kdl(entry.value())),
                    self.name().value(),
                    span,
                )
                .into());
            }
            let raw = entry.value().as_str(text, span)?;
            let toggle = Toggle::of(&raw);
            let value: T = T::of(toggle.name).ok_or_else(|| {
                crate::config::dispatch::Keys::unknown_value(T::NAMES, text, toggle.name, span)
            })?;
            if seen.contains(&value) {
                return Err(
                    ConfigError::duplicate_entry(text, &raw, self.name().value(), span).into(),
                );
            }
            seen.push(value);
            match toggle.add {
                true if !out.contains(&value) => out.push(value),
                true => {}
                false => out.retain(|kept| *kept != value),
            }
        }
        Toggle::required(self, text, seen.len())?;
        out.sort_by_key(|value| T::NAMES.iter().position(|(_, v)| v == value));
        Ok(out)
    }

    fn contained(&self, text: &str) -> Result<String> {
        let path = self.string(text, 0)?;
        match crate::fs::Contained::new(&path) {
            Some(_) => Ok(path),
            None => Err(ConfigError::escaping_file(text, &path, NodeExt::span(self)).into()),
        }
    }
}

/// Span bridging for a single [`KdlEntry`] (kdl's own miette 7 -> ours), so
/// value-level errors can point at the exact argument rather than the node.
pub(super) trait EntryExt {
    fn span(&self) -> SourceSpan;
}

impl EntryExt for KdlEntry {
    // The spelled-out type shows this is kdl's inherent method, not a recursion.
    #[allow(clippy::use_self)]
    fn span(&self) -> SourceSpan {
        let s = KdlEntry::span(self);
        SourceSpan::new(s.offset().into(), s.len())
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    /// An out-of-range port is rejected rather than truncated into a different,
    /// reachable one.
    #[test]
    fn an_out_of_range_ssh_port_is_rejected() {
        let err = Config::parse("deploy {\n  ssh {\n    host \"h\"\n    port 70000\n  }\n}")
            .unwrap_err()
            .to_string();
        assert!(err.contains("port"), "{err}");
    }

    #[test]
    fn a_plaintext_credential_host_is_rejected() {
        for kdl in [
            "announce {\n  standard {\n    handle \"a.example\"\n    pds \"http://evil.local\"\n  }\n}",
            "deploy {\n  s3 {\n    bucket \"b\"\n    endpoint \"http://evil.local\"\n  }\n}",
        ] {
            let err = Config::parse(kdl).unwrap_err().to_string();
            assert!(err.contains("not https"), "{kdl}: {err}");
        }
    }

    /// A host that merely *starts* with a loopback spelling is not loopback.
    #[test]
    fn userinfo_cannot_forge_a_loopback_host() {
        for host in [
            "http://localhost:9000@evil.com",
            "http://127.0.0.1@evil.com",
            "http://localhost.evil.com",
            "http://localhost@evil.com/path",
        ] {
            let kdl =
                format!("deploy {{\n  s3 {{\n    bucket \"b\"\n    endpoint \"{host}\"\n  }}\n}}");
            assert!(Config::parse(&kdl).is_err(), "accepted {host}");
        }
    }

    #[test]
    fn a_loopback_url_stays_plaintext() {
        let config = Config::parse(
            "deploy {\n  s3 {\n    bucket \"b\"\n    endpoint \"http://localhost:9000\"\n  }\n}",
        )
        .unwrap();
        assert_eq!(
            config.deploy.s3.expect("s3").endpoint.as_deref(),
            Some("http://localhost:9000")
        );
    }

    #[test]
    fn an_in_range_ssh_port_is_kept() {
        let config =
            Config::parse("deploy {\n  ssh {\n    host \"h\"\n    port 2222\n  }\n}").unwrap();
        assert_eq!(config.deploy.ssh.expect("ssh block").port, 2222);
    }
}
