//! Typed accessors over [`KdlNode`] and [`KdlEntry`]: the primitives every
//! config rule is written in terms of. The [`KdlValue`] half lives in
//! [`super::value`], the schema those primitives spell out in [`super::parse`].

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use crate::config::url::BaseUrl;
use crate::config::value::{Kdl, ValueExt};
use crate::error::{ConfigError, Result};
use crate::ui::Bytes;

/// The loopback interface, by the spellings a URL authority can name it.
struct Loopback;

impl Loopback {
    /// Whether a URL's post-scheme remainder names loopback.
    ///
    /// Userinfo is stripped at the *last* `@` first: the authority of
    /// `http://localhost:9000@evil.com` is `evil.com`, and matching the prefix
    /// would have shipped credentials there in cleartext.
    fn at(rest: &str) -> bool {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let host = host.rsplit_once(':').map_or(host, |(head, port)| {
            match port.chars().all(|c| c.is_ascii_digit()) {
                true => head,
                false => host,
            }
        });
        matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
    }
}

/// Typed reads of a [`KdlNode`]'s arguments and children, each erroring with a
/// span into the config text rather than coercing.
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
    fn url(&self, text: &str, idx: usize) -> Result<String>;
    fn base_url(&self, text: &str, idx: usize) -> Result<String>;
    /// A permalink template, or a piece of one, checked by [`Permalink::parse`]:
    /// an unknown placeholder, an unterminated `{`, and any `..` segment are
    /// errors at the span the author wrote.
    ///
    /// The single owner of that rule for the config surface, so the keys that
    /// are *parts* of an index's permalink (`paginate { mount }`, `prefix`) are
    /// held to it too. A `..` there escapes no file -- `Config::segments` drops
    /// one before anything is written -- but it does publish a page at a URL
    /// nobody asked for, out of a build that reported success.
    ///
    /// [`Permalink::parse`]: crate::config::Permalink::parse
    fn template(&self, text: &str, idx: usize) -> Result<String>;
    fn block(&self, text: &str) -> Result<&KdlDocument>;
    /// The node's `{ .. }` children parsed as `(id, item)` pairs, erroring on a
    /// duplicate id: the single dedup rule for collections, taxonomies, and
    /// profiles, where a repeated id would otherwise silently lose one side.
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
    fn widths(&self, text: &str) -> Result<Vec<u32>>;
    fn mapped<T: super::Named>(&self, text: &str) -> Result<Vec<T>>;
    /// A list of names over a fixed set, where `-name` removes one from
    /// `defaults` and a bare name adds one: `extensions "math" "-tables"`.
    ///
    /// The shape a setting takes when it has defaults worth keeping. Spelling
    /// the whole set out would mean a site that wants one extra silently loses
    /// every default it did not repeat, which is the trap `typst { features }`
    /// exists to avoid, generalised over any [`Named`](super::Named) table.
    fn toggled<T: super::Named>(&self, text: &str, defaults: &[T]) -> Result<Vec<T>>;
    /// This node's first argument as a file name that stays inside the output
    /// directory, judged by [`crate::fs::Contained`] (the one owner of that
    /// rule, shared with theme roots and inlined SVG paths).
    fn contained(&self, text: &str) -> Result<String>;
}

/// One name in a toggled list, and whether it is being added or removed.
///
/// The `-name` / `+name` / `name` grammar, in the one place that knows it. Two
/// keys are written in it -- `typst { features }` over an open set of names it
/// forwards, and any [`Named`](super::Named) list through
/// [`NodeExt::toggled`] -- and spelling the prefixes twice is how they would
/// come to disagree about `+`.
struct Toggle<'a> {
    name: &'a str,
    /// `false` for `-name`.
    add: bool,
}

impl<'a> Toggle<'a> {
    fn of(raw: &'a str) -> Self {
        match raw.strip_prefix('-') {
            Some(name) => Self { name, add: false },
            None => Self {
                name: raw.strip_prefix('+').unwrap_or(raw),
                add: true,
            },
        }
    }
}

impl NodeExt for KdlNode {
    /// Bridge kdl's `miette::SourceSpan` (its own miette 7) to ours.
    // `use_self` wants `Self::span`, which reads as a call to the very function
    // it sits in. It resolves to kdl's inherent method either way, but the type
    // name is what says so.
    #[allow(clippy::use_self)]
    fn span(&self) -> SourceSpan {
        let s = KdlNode::span(self);
        SourceSpan::new(s.offset().into(), s.len())
    }

    fn string(&self, text: &str, idx: usize) -> Result<String> {
        self.arg(text, idx)?.as_str(text, NodeExt::span(self))
    }

    fn arg(&self, text: &str, idx: usize) -> Result<&KdlValue> {
        match self.get(idx) {
            Some(value) => Ok(value),
            None => {
                Err(ConfigError::missing_arg(text, self.name().value(), NodeExt::span(self)).into())
            }
        }
    }

    /// A flag node's boolean: a bare node (`prune`) enables, a present argument
    /// must be a KDL boolean (`prune #false`); anything else is a type error,
    /// never a silent coercion.
    fn boolean(&self, text: &str, idx: usize) -> Result<bool> {
        match self.get(idx) {
            None => Ok(true),
            Some(value) => value.boolean(text, NodeExt::span(self)),
        }
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

    /// A byte size. An integer is bytes and is range-checked like any other
    /// count; a string is read by [`Bytes::parse`], the same rule that formats
    /// one, so a budget can be written in the units the build summary prints.
    fn size(&self, text: &str, idx: usize) -> Result<Bytes> {
        let span = NodeExt::span(self);
        let value = self.arg(text, idx)?;
        if let Some(written) = value.as_string() {
            return Bytes::parse(written)
                .ok_or_else(|| ConfigError::bad_size(text, written, span).into());
        }
        Ok(Bytes(self.count(text, idx)? as u64))
    }

    /// A base-URL argument, required to be `https://` unless it names the
    /// loopback interface.
    ///
    /// Credentials travel to these hosts: the app password in an atproto
    /// `createSession` body and every Bearer token after it, an AWS signature
    /// and its headers. A plaintext scheme puts them on the wire, and nothing
    /// downstream re-checks: `remote::tls()` keeps verification on and then
    /// hands the URL to `ureq` unexamined. Loopback stays allowed so a local
    /// MinIO or PDS on `http://localhost:9000` still works.
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
    /// Separate from [`url`](NodeExt::url) because the two answer different
    /// questions. That one guards a host credentials are sent to and demands
    /// https; this one guards the shape everything downstream joins onto, and a
    /// site served over plain http is the author's business.
    fn base_url(&self, text: &str, idx: usize) -> Result<String> {
        let value = self.string(text, idx)?;
        match BaseUrl::absolute(&value) {
            true => Ok(value),
            false => Err(ConfigError::not_absolute_url(text, &value, NodeExt::span(self)).into()),
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
                let span = NodeExt::span(child);
                // One argument is that value; several are a list. Only the
                // first used to be read, so `months "janvier" "février" ..`
                // kept January and dropped the year.
                let mut values = Vec::new();
                let mut index = 0;
                while let Some(arg) = child.get(index) {
                    values.push(arg.scalar(text, span)?);
                    index += 1;
                }
                let value = match values.len() {
                    // No argument at all: report it the way every other missing
                    // argument is reported.
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
            .map(|child| Ok((child.name().value().to_owned(), child.string(text, 0)?)))
            .collect()
    }

    fn features(&self, text: &str) -> Result<Vec<String>> {
        let positional: Vec<_> = self
            .entries()
            .iter()
            .filter(|e| e.name().is_none())
            .collect();
        if positional.is_empty() {
            return Err(
                ConfigError::missing_arg(text, self.name().value(), NodeExt::span(self)).into(),
            );
        }
        positional
            .iter()
            .map(|entry| {
                let span = EntryExt::span(*entry);
                let raw = entry.value().as_str(text, span)?;
                let toggle = Toggle::of(&raw);
                // `html` underpins the whole HTML pipeline, so it is always on
                // and refusing `-html` is clearer than silently keeping it.
                if !toggle.add && toggle.name == "html" {
                    return Err(ConfigError::feature_removal(text, toggle.name, span).into());
                }
                // Normalized: a leading `-` survives for `world.rs` to resolve
                // in order, and the optional `+` is dropped. This list is an
                // *open* set forwarded to typst, which is why it stays strings
                // rather than going through `toggled`.
                Ok(match toggle.add {
                    true => toggle.name.to_owned(),
                    false => format!("-{}", toggle.name),
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

    /// The positional integer arguments of a `widths 480 960 ..` node, each a
    /// pixel width in `1..=16384` (the max texture size browsers guarantee).
    /// Mirrors [`NodeExt::words`] for the integer case.
    fn widths(&self, text: &str) -> Result<Vec<u32>> {
        let span = NodeExt::span(self);
        self.entries()
            .iter()
            .filter(|e| e.name().is_none())
            .map(|e| e.value().bounded::<u32>(text, span, 1, 16384))
            .collect()
    }

    /// The node's positional string args mapped through `T`'s name table,
    /// erroring on the first unknown name with a nearest-match hint. The single
    /// source for parsing enum lists (feed formats, search formats/fields): the
    /// table on the enum drives both the mapping and the "valid names" in
    /// errors.
    fn mapped<T: super::Named>(&self, text: &str) -> Result<Vec<T>> {
        let mut out: Vec<T> = Vec::new();
        for entry in self.entries().iter().filter(|e| e.name().is_none()) {
            let span = EntryExt::span(entry);
            let value: T = entry.value().one(text, span)?;
            // a repeated entry would silently emit the output twice
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
        // Every entry, not just the positional ones: `extensions tables=#false`
        // is a plausible spelling (the config language has `key=value` lines
        // elsewhere), and quietly keeping the defaults while ignoring it is the
        // silent no-op this dispatch layer exists to prevent.
        let mut out = defaults.to_vec();
        let mut seen: Vec<T> = Vec::new();
        for entry in self.entries() {
            let span = EntryExt::span(entry);
            if let Some(key) = entry.name() {
                return Err(ConfigError::unexpected_argument(
                    text,
                    // Through `Kdl`, so the pair the message quotes back is the
                    // pair the author wrote rather than an unquoted lookalike.
                    &format!("{}={}", key.value(), Kdl(entry.value())),
                    self.name().value(),
                    span,
                )
                .into());
            }
            let raw = entry.value().as_str(text, span)?;
            let toggle = Toggle::of(&raw);
            // The same resolution and the same near-miss suggestion that naming
            // one of these anywhere else gives; only the `-`/`+` is ours.
            let value: T = T::of(toggle.name).ok_or_else(|| {
                crate::config::dispatch::Keys::unknown_value(T::NAMES, text, toggle.name, span)
            })?;
            // Naming one twice is a contradiction to resolve by reading, not by
            // silently taking the last word: `"math" "-math"` says two things.
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
        if seen.is_empty() {
            return Err(
                ConfigError::missing_arg(text, self.name().value(), NodeExt::span(self)).into(),
            );
        }
        // Canonical order, so a set is stored the way it is *named* rather than
        // the way it was reached. `"-tables" "tables"` and the defaults are the
        // same set and must hash the same: this value reaches the build
        // fingerprint, and a reorder that changes nothing would otherwise cost
        // a cold rebuild of the whole site.
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
    // As on `NodeExt::span` above: the spelled-out type is what shows this is
    // kdl's inherent method and not a recursive call.
    #[allow(clippy::use_self)]
    fn span(&self) -> SourceSpan {
        let s = KdlEntry::span(self);
        SourceSpan::new(s.offset().into(), s.len())
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    /// An out-of-range SSH port is rejected with the port diagnostic, not
    /// truncated into a different, reachable port (`70000` used to parse as
    /// `4464`).
    #[test]
    fn an_out_of_range_ssh_port_is_rejected() {
        let err = Config::parse("deploy {\n  ssh {\n    host \"h\"\n    port 70000\n  }\n}")
            .unwrap_err()
            .to_string();
        assert!(err.contains("port"), "{err}");
    }

    /// Credentials travel to `pds` and `endpoint`, so a plaintext scheme is
    /// rejected where the span can point at it, not silently honoured.
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

    /// A host that merely *starts* with a loopback spelling is not loopback:
    /// URL userinfo made `http://localhost:9000@evil.com` pass, which shipped
    /// the credentials to `evil.com` in cleartext.
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

    /// A local service is the one case plaintext is legitimate.
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
