//! An S3-compatible deploy backend, hand-rolled on `ureq` + [SigV4](super::sigv4)
//! to match the codebase's blocking, no-async HTTP. It reconciles a bucket with
//! the built `dist` directory: upload what changed, delete what the build no
//! longer produces.
//!
//! The moving parts are kept pure and tested — key encoding, the listing parse,
//! and the upload/delete [`plan`] — while the `ureq` calls stay a thin shell
//! around them. Change detection is stateless: S3 returns each object's ETag,
//! which for a single-part upload is the hex MD5 of its bytes, so a local file
//! whose MD5 matches the remote ETag is skipped without any local record.

use std::collections::BTreeMap;

use md5::{Digest, Md5};
use time::OffsetDateTime;

use super::sigv4::{self, Request, Signer};
use super::{Backend, Dist};
use crate::config::S3Config;
use crate::error::{DeployError, Result};
use crate::mime::Mime;
use crate::remote::Options;
use crate::ui::{Count, Ui};

/// The S3 deploy backend: resolves credentials, reconciles the bucket against
/// the built `dist`, and reports the plan. Holds only config; the live client is
/// built per run once credentials are in hand.
pub struct S3 {
    config: S3Config,
}

impl S3 {
    pub fn new(config: S3Config) -> Self {
        Self { config }
    }
}

impl Backend for S3 {
    fn name(&self) -> &'static str {
        "s3"
    }

    fn run(&self, dist: &Dist, opts: &Options, ui: &Ui) -> Result<()> {
        // The access key id is an identifier, read straight from the environment;
        // the secret key flows through the shared resolver so `--password`, stdin,
        // and the interactive prompt all work.
        let access_key = credential(ACCESS_KEY_ENV)?;
        let secret_key = opts.secret(SECRET_KEY_ENV, "AWS secret access key")?;
        let bucket = Bucket::new(&self.config, access_key, secret_key);

        let local = digests(dist)?;
        let remote = bucket.list()?;
        let plan = plan(&local, &remote, self.config.delete);
        report(ui, &plan, opts.dry_run);
        if opts.dry_run {
            return Ok(());
        }

        for key in &plan.uploads {
            bucket.put(key, &dist.read(key)?)?;
            ui.item(format_args!("↑ {key}"));
        }
        for key in &plan.deletes {
            bucket.delete(key)?;
            ui.item(format_args!("✕ {key}"));
        }
        ui.done(format_args!(
            "deployed to {} — {} uploaded, {} deleted",
            self.config.bucket,
            plan.uploads.len(),
            plan.deletes.len()
        ));
        Ok(())
    }
}

/// The MD5 digest of every built file, keyed by dist-relative path — each file is
/// read once, hashed, and dropped, so a large site never sits wholly in memory.
/// MD5 because that is exactly what an S3 ETag is for a single-part upload.
fn digests(dist: &Dist) -> Result<BTreeMap<String, String>> {
    dist.files
        .iter()
        .map(|rel| Ok((rel.clone(), md5_hex(&dist.read(rel)?))))
        .collect()
}

/// Report the reconcile plan: a one-line summary, prefixed on a dry run so the
/// preview reads as a preview.
fn report(ui: &Ui, plan: &Plan, dry_run: bool) {
    let lead = if dry_run { "dry run — would deploy " } else { "" };
    ui.detail(format_args!(
        "{lead}{} to upload, {} to delete, {} unchanged",
        Count::files(plan.uploads.len()),
        Count::files(plan.deletes.len()),
        plan.unchanged
    ));
}

/// Read a required credential from the environment, erroring with the variable's
/// name when it is unset or empty.
fn credential(var: &str) -> Result<String> {
    match std::env::var(var) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => Err(DeployError::MissingCredentials { var: var.to_owned() }.into()),
    }
}

/// Credential environment variables, in AWS's conventional names so existing CI
/// secrets and `~/.aws` tooling carry over.
pub const ACCESS_KEY_ENV: &str = "AWS_ACCESS_KEY_ID";
pub const SECRET_KEY_ENV: &str = "AWS_SECRET_ACCESS_KEY";

/// An S3-compatible bucket client.
pub struct Bucket {
    agent: ureq::Agent,
    access_key: String,
    secret_key: String,
    region: String,
    /// Key prefix every object is placed under (no leading/trailing slash).
    prefix: String,
    /// URL prefix objects hang off, no trailing slash — virtual-hosted for AWS
    /// (`https://bucket.s3.region.amazonaws.com`), path-style for a custom
    /// endpoint (`https://endpoint/bucket`).
    base: String,
    /// Host header the signature commits to.
    host: String,
    /// The signing URI path matching `base` — empty for virtual-hosted, `/bucket`
    /// for path-style.
    root: String,
}

impl Bucket {
    /// Build a client for `config` with credentials resolved from the
    /// environment. A custom `endpoint` selects path-style addressing (R2,
    /// MinIO); its absence targets AWS virtual-hosted addressing.
    pub fn new(config: &S3Config, access_key: String, secret_key: String) -> Self {
        let (base, host, root) = match &config.endpoint {
            Some(endpoint) => {
                let endpoint = endpoint.trim_end_matches('/');
                let host = endpoint.split_once("://").map_or(endpoint, |(_, h)| h).to_owned();
                (format!("{endpoint}/{}", config.bucket), host, format!("/{}", config.bucket))
            }
            None => {
                let host = format!("{}.s3.{}.amazonaws.com", config.bucket, config.region);
                (format!("https://{host}"), host, String::new())
            }
        };
        Self {
            agent: ureq::agent(),
            access_key,
            secret_key,
            region: config.region.clone(),
            prefix: config.prefix.trim_matches('/').to_owned(),
            base,
            host,
            root,
        }
    }

    /// Every object currently under the prefix, keyed by object key with its
    /// ETag, following continuation tokens to the end.
    pub fn list(&self) -> Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![("list-type", "2".to_owned())];
            if !self.prefix.is_empty() {
                query.push(("prefix", format!("{}/", self.prefix)));
            }
            if let Some(token) = &token {
                query.push(("continuation-token", token.clone()));
            }
            let body = self.send("GET", &format!("{}/", self.root), &canonical_query(&query), &[])?;
            let listing = parse_listing(&body)?;
            out.extend(listing.objects.into_iter().map(|(key, etag)| (self.relative(key), etag)));
            match listing.next {
                Some(next) => token = Some(next),
                None => break,
            }
        }
        Ok(out)
    }

    /// Upload `body` to `key` (a relative dist path), setting its content type
    /// from the extension.
    pub fn put(&self, key: &str, body: &[u8]) -> Result<()> {
        let content_type = Mime::of(key).header();
        self.write("PUT", &self.object(key), body, Some(&content_type))?;
        Ok(())
    }

    /// Delete the object at `key` (a relative dist path).
    pub fn delete(&self, key: &str) -> Result<()> {
        self.write("DELETE", &self.object(key), &[], None)?;
        Ok(())
    }

    /// The signing URI for an object at relative `key`: the root, the prefix, and
    /// the URI-encoded key.
    fn object(&self, key: &str) -> String {
        format!("{}/{}", self.root, encode(&object_key(&self.prefix, key)))
    }

    /// Strip the configured prefix from a listed object key, so the whole client
    /// speaks one namespace — dist-relative paths — with the prefix an internal
    /// detail of addressing.
    fn relative(&self, key: String) -> String {
        if self.prefix.is_empty() {
            return key;
        }
        key.strip_prefix(&format!("{}/", self.prefix))
            .map(str::to_owned)
            .unwrap_or(key)
    }

    /// A signed GET returning the response body as a string (listings).
    fn send(&self, method: &'static str, uri: &str, query: &str, body: &[u8]) -> Result<String> {
        let url = if query.is_empty() {
            self.base_of(uri)
        } else {
            format!("{}?{query}", self.base_of(uri))
        };
        let auth = self.authorize(method, uri, query, body);
        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &auth.header)
            .header("x-amz-date", &auth.timestamp)
            .header("x-amz-content-sha256", &auth.payload_hash)
            .call()
            .map_err(DeployError::from)?;
        self.check(method, uri, response.status().as_u16(), &mut response)?;
        Ok(response.body_mut().read_to_string().unwrap_or_default())
    }

    /// A signed PUT (with a body) or DELETE (without). ureq types the two builders
    /// differently, so each drives its own call.
    fn write(&self, method: &'static str, uri: &str, body: &[u8], content_type: Option<&str>) -> Result<()> {
        let url = self.base_of(uri);
        let auth = self.authorize(method, uri, "", body);
        let mut response = if method == "DELETE" {
            self.agent
                .delete(&url)
                .header("Authorization", &auth.header)
                .header("x-amz-date", &auth.timestamp)
                .header("x-amz-content-sha256", &auth.payload_hash)
                .call()
        } else {
            let mut request = self
                .agent
                .put(&url)
                .header("Authorization", &auth.header)
                .header("x-amz-date", &auth.timestamp)
                .header("x-amz-content-sha256", &auth.payload_hash);
            if let Some(content_type) = content_type {
                request = request.header("Content-Type", content_type);
            }
            request.send(body)
        }
        .map_err(DeployError::from)?;
        self.check(method, uri, response.status().as_u16(), &mut response)
    }

    /// The full URL for a signing `uri` (which already carries the root/prefix).
    fn base_of(&self, uri: &str) -> String {
        // `base` ends at the host (+ bucket for path-style); `uri` starts at the
        // same root, so trim the root from `base`'s tail is unnecessary — `uri`
        // begins after the host. Compose host-authority + uri.
        let authority = self.base.strip_suffix(&self.root).unwrap_or(&self.base);
        format!("{authority}{uri}")
    }

    /// Sign a request, returning the header trio to attach.
    fn authorize(&self, method: &str, uri: &str, query: &str, body: &[u8]) -> Authorization {
        let timestamp = amz_timestamp(OffsetDateTime::now_utc());
        let payload_hash = sigv4::sha256_hex(body);
        let signer = Signer {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            service: "s3",
            timestamp: &timestamp,
        };
        let header = signer.sign(&Request {
            method,
            host: &self.host,
            uri,
            query,
            headers: &[("x-amz-content-sha256", &payload_hash)],
            payload_hash: &payload_hash,
        });
        Authorization { header, timestamp, payload_hash }
    }

    /// Turn a non-2xx status into a [`DeployError::Request`] carrying the host's
    /// own error body.
    fn check(
        &self,
        operation: &'static str,
        uri: &str,
        status: u16,
        response: &mut ureq::http::Response<ureq::Body>,
    ) -> Result<()> {
        if (200..300).contains(&status) {
            return Ok(());
        }
        let message = response.body_mut().read_to_string().unwrap_or_default();
        Err(DeployError::Request {
            operation,
            key: uri.to_owned(),
            status,
            message: message.trim().to_owned(),
        }
        .into())
    }
}

/// The signed-request headers to attach.
struct Authorization {
    header: String,
    timestamp: String,
    payload_hash: String,
}

/// A parsed bucket listing: the objects on this page and the continuation token
/// for the next, if the result was truncated.
struct Listing {
    objects: Vec<(String, String)>,
    next: Option<String>,
}

/// What a reconcile will do to the remote.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Keys to upload (new or changed).
    pub uploads: Vec<String>,
    /// Keys to delete (present remotely, gone locally).
    pub deletes: Vec<String>,
    /// How many objects are already up to date.
    pub unchanged: usize,
}

/// Diff the local digests against the remote listing, both keyed by dist-relative
/// path. A file uploads when its MD5 differs from the remote ETag (or it is new);
/// an object deletes when the build no longer produces it and `delete` is on;
/// everything else is unchanged.
pub fn plan(local: &BTreeMap<String, String>, remote: &BTreeMap<String, String>, delete: bool) -> Plan {
    let mut out = Plan::default();
    for (key, digest) in local {
        match remote.get(key) {
            Some(etag) if unquote(etag).eq_ignore_ascii_case(digest) => out.unchanged += 1,
            _ => out.uploads.push(key.clone()),
        }
    }
    if delete {
        out.deletes = remote.keys().filter(|key| !local.contains_key(*key)).cloned().collect();
    }
    out
}

/// The object key for a dist-relative path under `prefix`: forward-slashed, no
/// leading slash, prefix folded in.
fn object_key(prefix: &str, path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.trim_start_matches('/');
    if prefix.is_empty() {
        path.to_owned()
    } else {
        format!("{prefix}/{path}")
    }
}

/// Percent-encode a key for a URL path: every byte except the unreserved set and
/// the path separator `/`, uppercase hex, per the S3 signing rules.
fn encode(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for byte in key.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Canonical query string: each name and value URI-encoded, sorted by name.
fn canonical_query(params: &[(&str, String)]) -> String {
    let mut params: Vec<(String, String)> =
        params.iter().map(|(name, value)| (encode_component(name), encode_component(value))).collect();
    params.sort();
    params.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>().join("&")
}

/// Percent-encode a query component: unreserved bytes pass, everything else
/// (including `/`) is encoded.
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Parse a ListObjectsV2 XML response into its objects and continuation token.
fn parse_listing(xml: &str) -> Result<Listing> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|e| DeployError::Listing { message: e.to_string() })?;
    let text = |node: roxmltree::Node, tag: &str| {
        node.children().find(|c| c.has_tag_name(tag)).and_then(|c| c.text()).map(str::to_owned)
    };
    let objects = document
        .descendants()
        .filter(|node| node.has_tag_name("Contents"))
        .filter_map(|node| Some((text(node, "Key")?, unquote(&text(node, "ETag")?).to_owned())))
        .collect();
    let next = document
        .descendants()
        .find(|node| node.has_tag_name("NextContinuationToken"))
        .and_then(|node| node.text())
        .map(str::to_owned);
    Ok(Listing { objects, next })
}

/// Strip the surrounding quotes S3 wraps an ETag in.
fn unquote(etag: &str) -> &str {
    etag.trim_matches('"')
}

/// Lowercase hex MD5 — the ETag S3 assigns a single-part upload.
fn md5_hex(bytes: &[u8]) -> String {
    Md5::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Format an instant as SigV4's `YYYYMMDDTHHMMSSZ`.
fn amz_timestamp(now: OffsetDateTime) -> String {
    let (year, month, day) = (now.year(), now.month() as u8, now.day());
    let (hour, minute, second) = (now.hour(), now.minute(), now.second());
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(endpoint: Option<&str>, prefix: &str) -> S3Config {
        S3Config {
            bucket: "my-site".into(),
            endpoint: endpoint.map(String::from),
            region: "us-east-1".into(),
            prefix: prefix.into(),
            delete: true,
        }
    }

    fn bucket(endpoint: Option<&str>, prefix: &str) -> Bucket {
        Bucket::new(&config(endpoint, prefix), "AKID".into(), "secret".into())
    }

    #[test]
    fn aws_addressing_is_virtual_hosted() {
        let b = bucket(None, "");
        assert_eq!(b.host, "my-site.s3.us-east-1.amazonaws.com");
        assert_eq!(b.base, "https://my-site.s3.us-east-1.amazonaws.com");
        assert_eq!(b.root, "");
        assert_eq!(b.object("posts/a.html"), "/posts/a.html");
    }

    #[test]
    fn custom_endpoint_is_path_style() {
        let b = bucket(Some("https://acct.r2.cloudflarestorage.com"), "");
        assert_eq!(b.host, "acct.r2.cloudflarestorage.com");
        assert_eq!(b.base, "https://acct.r2.cloudflarestorage.com/my-site");
        assert_eq!(b.root, "/my-site");
        assert_eq!(b.object("a.html"), "/my-site/a.html");
        // the full URL recomposes to the object.
        assert_eq!(b.base_of(&b.object("a.html")), "https://acct.r2.cloudflarestorage.com/my-site/a.html");
    }

    #[test]
    fn prefix_folds_into_object_keys() {
        let b = bucket(None, "/sub/dir/");
        assert_eq!(b.prefix, "sub/dir");
        assert_eq!(b.object("a.html"), "/sub/dir/a.html");
    }

    #[test]
    fn relative_strips_the_prefix_from_listed_keys() {
        let b = bucket(None, "sub/dir");
        assert_eq!(b.relative("sub/dir/a.html".into()), "a.html");
        // A key outside the prefix is passed through unchanged.
        assert_eq!(b.relative("other/a.html".into()), "other/a.html");
        // With no prefix, keys are already relative.
        assert_eq!(bucket(None, "").relative("a.html".into()), "a.html");
    }

    #[test]
    fn object_key_normalizes() {
        assert_eq!(object_key("", "posts/a.html"), "posts/a.html");
        assert_eq!(object_key("", "/posts/a.html"), "posts/a.html");
        assert_eq!(object_key("site", "posts/a.html"), "site/posts/a.html");
        assert_eq!(object_key("site", "a\\b.html"), "site/a/b.html");
    }

    #[test]
    fn encode_keeps_slashes_and_unreserved() {
        assert_eq!(encode("posts/a-b_c.html"), "posts/a-b_c.html");
        assert_eq!(encode("a b.html"), "a%20b.html");
        assert_eq!(encode("caf\u{e9}.html"), "caf%C3%A9.html");
        assert_eq!(encode("a+b&c.html"), "a%2Bb%26c.html");
    }

    #[test]
    fn canonical_query_sorts_and_encodes() {
        let query = canonical_query(&[
            ("prefix", "a/b c".into()),
            ("list-type", "2".into()),
        ]);
        assert_eq!(query, "list-type=2&prefix=a%2Fb%20c");
    }

    #[test]
    fn md5_matches_the_known_empty_vector() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn listing_parses_keys_etags_and_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <ListBucketResult>
              <IsTruncated>true</IsTruncated>
              <Contents><Key>a.html</Key><ETag>&quot;abc123&quot;</ETag><Size>10</Size></Contents>
              <Contents><Key>b/c.css</Key><ETag>"def456"</ETag><Size>20</Size></Contents>
              <NextContinuationToken>TOKEN==</NextContinuationToken>
            </ListBucketResult>"#;
        let listing = parse_listing(xml).unwrap();
        assert_eq!(
            listing.objects,
            vec![("a.html".into(), "abc123".into()), ("b/c.css".into(), "def456".into())]
        );
        assert_eq!(listing.next.as_deref(), Some("TOKEN=="));
    }

    #[test]
    fn listing_without_token_ends() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";
        let listing = parse_listing(xml).unwrap();
        assert!(listing.objects.is_empty());
        assert_eq!(listing.next, None);
    }

    fn local(pairs: &[(&str, &[u8])]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), md5_hex(v))).collect()
    }

    fn remote(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn plan_uploads_new_and_changed_skips_matching() {
        let local = local(&[("new.html", b"n"), ("same.html", b"s"), ("changed.html", b"x")]);
        let remote = remote(&[
            ("same.html", &md5_hex(b"s")),
            ("changed.html", &md5_hex(b"old")),
        ]);
        let plan = plan(&local, &remote, true);
        assert_eq!(plan.uploads, vec!["changed.html".to_string(), "new.html".to_string()]);
        assert_eq!(plan.unchanged, 1);
    }

    #[test]
    fn plan_deletes_orphans_only_when_enabled() {
        let local = local(&[("keep.html", b"k")]);
        let remote = remote(&[("keep.html", &md5_hex(b"k")), ("gone.html", "whatever")]);

        let with_delete = plan(&local, &remote, true);
        assert_eq!(with_delete.deletes, vec!["gone.html".to_string()]);
        assert_eq!(with_delete.unchanged, 1);

        let without_delete = plan(&local, &remote, false);
        assert!(without_delete.deletes.is_empty());
    }

    #[test]
    fn plan_matches_quoted_etags_case_insensitively() {
        let local = local(&[("a.html", b"a")]);
        let remote = remote(&[("a.html", &format!("\"{}\"", md5_hex(b"a").to_uppercase()))]);
        assert_eq!(plan(&local, &remote, true).unchanged, 1);
    }

    #[test]
    fn amz_timestamp_formats_utc() {
        let t = OffsetDateTime::from_unix_timestamp(1_440_938_160).unwrap(); // 2015-08-30T12:36:00Z
        assert_eq!(amz_timestamp(t), "20150830T123600Z");
    }
}
