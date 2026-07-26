//! An S3-compatible deploy backend, hand-rolled on `ureq` + [SigV4](super::sigv4)
//! to match the codebase's blocking, no-async HTTP. It reconciles a bucket with
//! the built `dist` directory: upload what changed, delete what the build no
//! longer produces.
//!
//! The moving parts are kept pure and tested (key encoding, the listing parse,
//! and the upload/delete [`Plan`]) while the `ureq` calls stay a thin shell
//! around them. Change detection is stateless: S3 returns each object's ETag,
//! which for a single-part upload is the hex MD5 of its bytes, so a local file
//! whose MD5 matches the remote ETag is skipped without any local record.

use std::collections::BTreeMap;

use md5::{Digest, Md5};
use time::OffsetDateTime;

use super::sigv4::{Request, Signer};
use super::{Backend, Dist, Listed, Plan};
use crate::config::S3Config;
use crate::error::{DeployError, Result};
use crate::mime::Mime;
use crate::remote::Options;
use crate::ui::Ui;

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

impl Backend<Dist> for S3 {
    fn name(&self) -> &'static str {
        "s3"
    }

    fn run(&self, dist: &Dist, opts: &Options, ui: &Ui) -> Result<()> {
        // The access key id is an identifier, read straight from the environment;
        // the secret key flows through the shared resolver so `--password`, stdin,
        // and the interactive prompt all work.
        let access_key = Self::credential(ACCESS_KEY_ENV)?;
        let secret_key = opts.secret(SECRET_KEY_ENV, "AWS secret access key")?;
        let bucket = Bucket::new(&self.config, access_key, secret_key, Self::session_token());

        let local = dist.digests(Bucket::etag)?;
        let plan = Plan::compute(&local, &bucket.list()?, self.config.delete);
        plan.preview(ui, opts.dry_run);
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
        plan.done(ui, &self.config.bucket);
        Ok(())
    }
}

impl S3 {
    /// Read a required credential from the environment, erroring with the
    /// variable's name when it is unset or empty.
    /// The session token accompanying temporary credentials, if any.
    fn session_token() -> Option<String> {
        std::env::var(SESSION_TOKEN_ENV)
            .ok()
            .filter(|token| !token.is_empty())
    }

    fn credential(var: &str) -> Result<String> {
        match std::env::var(var) {
            Ok(value) if !value.is_empty() => Ok(value),
            _ => Err(DeployError::MissingCredentials {
                var: var.to_owned(),
            }
            .into()),
        }
    }
}

/// Credential environment variables, in AWS's conventional names so existing CI
/// secrets and `~/.aws` tooling carry over.
pub const ACCESS_KEY_ENV: &str = "AWS_ACCESS_KEY_ID";
pub const SECRET_KEY_ENV: &str = "AWS_SECRET_ACCESS_KEY";

/// Session token for temporary credentials. Set by every mechanism that issues
/// them: GitHub OIDC, EC2/ECS instance roles, `aws sso login`, `sts
/// assume-role`. Without sending it, those credentials produce a
/// perfectly-formed signature the server rejects as `SignatureDoesNotMatch`.
pub const SESSION_TOKEN_ENV: &str = "AWS_SESSION_TOKEN";

/// An S3-compatible bucket client.
pub struct Bucket {
    agent: ureq::Agent,
    access_key: String,
    secret_key: String,
    /// Present only for temporary credentials; signed and sent as
    /// `x-amz-security-token`.
    token: Option<String>,
    region: String,
    /// Key prefix every object is placed under (no leading/trailing slash).
    prefix: String,
    /// Scheme and host a request URL hangs off, no trailing slash:
    /// `https://bucket.s3.region.amazonaws.com` for AWS virtual-hosting,
    /// `https://endpoint` for a custom endpoint. A signing URI is appended to it.
    authority: String,
    /// Host header the signature commits to.
    host: String,
    /// The leading path every signing URI carries: empty for virtual-hosted,
    /// `/bucket` for path-style.
    root: String,
}

impl Bucket {
    /// Build a client for `config` with credentials resolved from the
    /// environment. A custom `endpoint` selects path-style addressing (R2,
    /// MinIO); its absence targets AWS virtual-hosted addressing.
    pub fn new(
        config: &S3Config,
        access_key: String,
        secret_key: String,
        token: Option<String>,
    ) -> Self {
        let (authority, host, root) = match &config.endpoint {
            Some(endpoint) => {
                let endpoint = endpoint.trim_end_matches('/');
                let host = endpoint
                    .split_once("://")
                    .map_or(endpoint, |(_, h)| h)
                    .to_owned();
                (endpoint.to_owned(), host, format!("/{}", config.bucket))
            }
            None => {
                let host = format!("{}.s3.{}.amazonaws.com", config.bucket, config.region);
                (format!("https://{host}"), host, String::new())
            }
        };
        Self {
            agent: ureq::Agent::config_builder()
                .tls_config(crate::remote::tls())
                .build()
                .into(),
            access_key,
            secret_key,
            token,
            region: config.region.clone(),
            prefix: config.prefix.trim_matches('/').to_owned(),
            authority,
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
            let body = self.send(
                "GET",
                &format!("{}/", self.root),
                &Self::canonical_query(&query),
                &[],
            )?;
            let listing = Listing::parse(&body)?;
            out.extend(
                listing
                    .objects
                    .into_iter()
                    .map(|(key, etag)| (self.relative(key), etag)),
            );
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
        format!(
            "{}/{}",
            self.root,
            Self::encode(&Self::object_key(&self.prefix, key), true)
        )
    }

    /// Strip the configured prefix from a listed object key, so the whole client
    /// speaks one namespace, dist-relative paths, with the prefix an internal
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
        let url = match query.is_empty() {
            true => self.url(uri),
            false => format!("{}?{query}", self.url(uri)),
        };
        let auth = self.authorize(method, uri, query, body);
        let mut response = self
            .signed(self.agent.get(&url), &auth)
            .call()
            .map_err(DeployError::from)?;
        self.check(method, uri, response.status().as_u16(), &mut response)?;
        Ok(response.body_mut().read_to_string().unwrap_or_default())
    }

    /// A signed PUT (with a body) or DELETE (without). ureq types the two builders
    /// differently, so each drives its own call.
    fn write(
        &self,
        method: &'static str,
        uri: &str,
        body: &[u8],
        content_type: Option<&str>,
    ) -> Result<()> {
        let url = self.url(uri);
        let auth = self.authorize(method, uri, "", body);
        let mut response = if method == "DELETE" {
            self.signed(self.agent.delete(&url), &auth).call()
        } else {
            let mut request = self.signed(self.agent.put(&url), &auth);
            if let Some(content_type) = content_type {
                request = request.header("Content-Type", content_type);
            }
            request.send(body)
        }
        .map_err(DeployError::from)?;
        self.check(method, uri, response.status().as_u16(), &mut response)
    }

    /// Attach the SigV4 authorization header trio to any request builder.
    fn signed<Any>(
        &self,
        request: ureq::RequestBuilder<Any>,
        auth: &Authorization,
    ) -> ureq::RequestBuilder<Any> {
        let request = request
            .header("Authorization", &auth.header)
            .header("x-amz-date", &auth.timestamp)
            .header("x-amz-content-sha256", &auth.payload_hash);
        match &self.token {
            Some(token) => request.header(TOKEN_HEADER, token),
            None => request,
        }
    }

    /// The full URL for a signing `uri` (which already carries the root/prefix).
    fn url(&self, uri: &str) -> String {
        format!("{}{uri}", self.authority)
    }

    /// Sign a request, returning the header trio to attach.
    fn authorize(&self, method: &str, uri: &str, query: &str, body: &[u8]) -> Authorization {
        let timestamp = Self::timestamp(OffsetDateTime::now_utc());
        let payload_hash = Signer::sha256_hex(body);
        let signer = Signer {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            service: "s3",
            timestamp: &timestamp,
        };
        // The session token is part of the signature, not just a header: a
        // signature computed without it is rejected.
        let mut headers = vec![("x-amz-content-sha256", payload_hash.as_str())];
        if let Some(token) = &self.token {
            headers.push((TOKEN_HEADER, token.as_str()));
        }
        let header = signer.sign(&Request {
            method,
            host: &self.host,
            uri,
            query,
            headers: &headers,
            payload_hash: &payload_hash,
        });
        Authorization {
            header,
            timestamp,
            payload_hash,
        }
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
        let body = response.body_mut().read_to_string().unwrap_or_default();
        Err(DeployError::request(operation, uri, status, &body).into())
    }
}

/// The header carrying a temporary credential's session token, signed and sent
/// together so the two can never disagree.
const TOKEN_HEADER: &str = "x-amz-security-token";

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

/// Wire-format helpers: key normalization, encoding, and the values a request
/// signs with. Kept as associated functions so they stay pure and testable while
/// living under the client they serve.
impl Bucket {
    /// The object key for a dist-relative path under `prefix`: forward-slashed,
    /// no leading slash, prefix folded in.
    fn object_key(prefix: &str, path: &str) -> String {
        let path = path.replace('\\', "/");
        let path = path.trim_start_matches('/');
        if prefix.is_empty() {
            path.to_owned()
        } else {
            format!("{prefix}/{path}")
        }
    }

    /// Percent-encode per the S3 signing rules: unreserved bytes pass through,
    /// everything else becomes uppercase `%XX`. A path keeps its `/` separators;
    /// a query component encodes them too.
    fn encode(value: &str, keep_slash: bool) -> String {
        let mut out = String::with_capacity(value.len());
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(byte as char)
                }
                b'/' if keep_slash => out.push('/'),
                _ => out.push_str(&format!("%{byte:02X}")),
            }
        }
        out
    }

    /// Canonical query string: each name and value URI-encoded, sorted by name.
    fn canonical_query(params: &[(&str, String)]) -> String {
        let mut params: Vec<(String, String)> = params
            .iter()
            .map(|(name, value)| (Self::encode(name, false), Self::encode(value, false)))
            .collect();
        params.sort();
        params
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// The ETag S3 assigns a single-part upload: the lowercase hex MD5 of `bytes`.
    fn etag(bytes: &[u8]) -> String {
        Signer::hex(&Md5::digest(bytes))
    }

    /// Format an instant as SigV4's `YYYYMMDDTHHMMSSZ`.
    fn timestamp(now: OffsetDateTime) -> String {
        let (year, month, day) = (now.year(), now.month() as u8, now.day());
        let (hour, minute, second) = (now.hour(), now.minute(), now.second());
        format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
    }
}

impl Listing {
    /// Parse a ListObjectsV2 XML response into its objects and continuation token.
    fn parse(xml: &str) -> Result<Listing> {
        let document = roxmltree::Document::parse(xml).map_err(|e| DeployError::Listing {
            message: e.to_string(),
        })?;
        let text = |node: roxmltree::Node, tag: &str| {
            node.children()
                .find(|c| c.has_tag_name(tag))
                .and_then(|c| c.text())
                .map(str::to_owned)
        };
        let objects = document
            .descendants()
            .filter(|node| node.has_tag_name("Contents"))
            .filter_map(|node| {
                // The key came off the network and feeds a delete request, so
                // it only leaves here as a `Listed`.
                let key = Listed::try_from(text(node, "Key")?.as_str()).ok()?;
                let etag = Self::unquote(&text(node, "ETag")?).to_owned();
                Some((key.into_string(), etag))
            })
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
        Bucket::new(
            &config(endpoint, prefix),
            "AKID".into(),
            "secret".into(),
            None,
        )
    }

    #[test]
    fn aws_addressing_is_virtual_hosted() {
        let b = bucket(None, "");
        assert_eq!(b.host, "my-site.s3.us-east-1.amazonaws.com");
        assert_eq!(b.authority, "https://my-site.s3.us-east-1.amazonaws.com");
        assert_eq!(b.root, "");
        assert_eq!(b.object("posts/a.html"), "/posts/a.html");
        assert_eq!(
            b.url(&b.object("posts/a.html")),
            "https://my-site.s3.us-east-1.amazonaws.com/posts/a.html"
        );
    }

    #[test]
    fn custom_endpoint_is_path_style() {
        let b = bucket(Some("https://acct.r2.cloudflarestorage.com"), "");
        assert_eq!(b.host, "acct.r2.cloudflarestorage.com");
        assert_eq!(b.authority, "https://acct.r2.cloudflarestorage.com");
        assert_eq!(b.root, "/my-site");
        assert_eq!(b.object("a.html"), "/my-site/a.html");
        // the full URL recomposes to the object.
        assert_eq!(
            b.url(&b.object("a.html")),
            "https://acct.r2.cloudflarestorage.com/my-site/a.html"
        );
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
        assert_eq!(Bucket::object_key("", "posts/a.html"), "posts/a.html");
        assert_eq!(Bucket::object_key("", "/posts/a.html"), "posts/a.html");
        assert_eq!(
            Bucket::object_key("site", "posts/a.html"),
            "site/posts/a.html"
        );
        assert_eq!(Bucket::object_key("site", "a\\b.html"), "site/a/b.html");
    }

    #[test]
    fn encode_keeps_slashes_when_asked_and_escapes_the_rest() {
        assert_eq!(Bucket::encode("posts/a-b_c.html", true), "posts/a-b_c.html");
        assert_eq!(Bucket::encode("a b.html", true), "a%20b.html");
        assert_eq!(Bucket::encode("caf\u{e9}.html", true), "caf%C3%A9.html");
        assert_eq!(Bucket::encode("a+b&c.html", true), "a%2Bb%26c.html");
        // A query component encodes the slash too.
        assert_eq!(Bucket::encode("a/b", false), "a%2Fb");
    }

    #[test]
    fn canonical_query_sorts_and_encodes() {
        let query =
            Bucket::canonical_query(&[("prefix", "a/b c".into()), ("list-type", "2".into())]);
        assert_eq!(query, "list-type=2&prefix=a%2Fb%20c");
    }

    #[test]
    fn md5_matches_the_known_empty_vector() {
        assert_eq!(Bucket::etag(b""), "d41d8cd98f00b204e9800998ecf8427e");
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
        let listing = Listing::parse(xml).unwrap();
        assert_eq!(
            listing.objects,
            vec![
                ("a.html".into(), "abc123".into()),
                ("b/c.css".into(), "def456".into())
            ]
        );
        assert_eq!(listing.next.as_deref(), Some("TOKEN=="));
    }

    #[test]
    fn listing_without_token_ends() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";
        let listing = Listing::parse(xml).unwrap();
        assert!(listing.objects.is_empty());
        assert_eq!(listing.next, None);
    }

    #[test]
    fn amz_timestamp_formats_utc() {
        let t = OffsetDateTime::from_unix_timestamp(1_440_938_160).unwrap(); // 2015-08-30T12:36:00Z
        assert_eq!(Bucket::timestamp(t), "20150830T123600Z");
    }
}
