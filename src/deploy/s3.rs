//! An S3-compatible deploy backend on `ureq` + [SigV4](super::sigv4),
//! reconciling a bucket with the built `dist` against the ETag S3 reports.

use std::path::Path;

use md5::{Digest as _, Md5};
use time::OffsetDateTime;

use super::digest::Digest;
use super::sigv4::{DATE_HEADER, Request, Signer};
use super::{Backend, Digests, Dist, Inventory, Listed, Store};
use crate::config::{CacheControl, S3Config, Slashed};
use crate::error::deploy::{Method, Required};
use crate::error::warning::PlaintextEndpoint;
use crate::error::{DeployError, Result};
use crate::mime::Mime;
use crate::remote::Options;
use crate::ui::Ui;

/// The S3 deploy backend, holding only config; the live client is built per run
/// once credentials are in hand.
pub struct S3 {
    config: S3Config,
    /// Site-wide rather than per-destination, so it cannot disagree with what
    /// `_headers` states.
    cache: CacheControl,
    assets: Fingerprinted,
}

/// Whether a build content-addresses its assets, and where they live.
#[derive(Debug, Clone)]
pub struct Fingerprinted {
    pub prefix: String,
    pub hashed: bool,
}

impl S3 {
    pub fn new(config: S3Config, cache: CacheControl, assets: Fingerprinted) -> Self {
        Self {
            config,
            cache,
            assets,
        }
    }
}

impl Backend<Dist> for S3 {
    fn name(&self) -> &'static str {
        "s3"
    }

    fn run(&self, dist: &Dist, opts: &Options, ui: &Ui) -> Result<()> {
        if let Some(warning) = Self::plaintext(self.config.endpoint.as_deref()) {
            ui.warn(warning);
        }
        let access_key = Self::credential(ACCESS_KEY_ENV)?;
        let secret_key = opts.secret(SECRET_KEY_ENV, "AWS secret access key")?;
        let bucket = Bucket::new(
            &self.config,
            self.cache.clone(),
            self.assets.clone(),
            access_key,
            secret_key,
            Self::session_token(),
        );
        dist.reconcile(&bucket, self.config.delete, opts, ui)
    }
}

impl S3 {
    /// An empty `bucket` is not a default: it would sign every request against
    /// an authority nobody meant. `bucket` and `region` are both spliced into
    /// that authority, so neither may carry anything a host name cannot.
    pub(super) fn check(config: &S3Config) -> Result<()> {
        if config.bucket.trim().is_empty() {
            return Err(DeployError::required(Required::S3Bucket).into());
        }
        for (setting, value) in [
            ("deploy { s3 { bucket } }", config.bucket.as_str()),
            ("deploy { s3 { region } }", config.region()),
        ] {
            if !Self::names_a_host(value) {
                return Err(DeployError::not_a_name(setting, value).into());
            }
        }
        Ok(())
    }

    /// Whether `value` can stand in a host name: the characters that would end
    /// the authority and point the signed request somewhere else are refused.
    fn names_a_host(value: &str) -> bool {
        !value.is_empty()
            && !value
                .chars()
                .any(|c| "/?#@:\\".contains(c) || c.is_whitespace() || c.is_control())
    }

    /// Whether `endpoint` is plain HTTP, so the signed request travels in
    /// clear; reported rather than refused, since MinIO on `localhost` exists.
    fn plaintext(endpoint: Option<&str>) -> Option<PlaintextEndpoint> {
        let endpoint = endpoint?;
        endpoint.starts_with("http://").then(|| PlaintextEndpoint {
            setting: "deploy { s3 { endpoint } }",
            url: endpoint.to_owned(),
            secret: "the signed request, session token included",
        })
    }

    /// The session token accompanying temporary credentials, `None` without
    /// them.
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

/// AWS's conventional names, so existing CI secrets and `~/.aws` tooling carry
/// over.
pub const ACCESS_KEY_ENV: &str = "AWS_ACCESS_KEY_ID";
pub const SECRET_KEY_ENV: &str = crate::config::Secrets::S3_SECRET_KEY;

/// Session token for temporary credentials, without which they produce a
/// well-formed signature the server rejects as `SignatureDoesNotMatch`.
pub const SESSION_TOKEN_ENV: &str = crate::config::Secrets::S3_SESSION_TOKEN;

pub struct Bucket {
    agent: ureq::Agent,
    name: String,
    access_key: String,
    secret_key: String,
    /// `None` for long-lived credentials; otherwise signed and sent as
    /// `x-amz-security-token`.
    token: Option<String>,
    region: String,
    /// No leading or trailing slash.
    prefix: String,
    /// Scheme and host a request URL hangs off, no trailing slash.
    authority: String,
    host: String,
    /// The leading path every signing URI carries: empty for virtual-hosted,
    /// `/bucket` for path-style.
    root: String,
    cache: CacheControl,
    assets: Fingerprinted,
    /// How many objects the reconcile transfers at once.
    concurrency: Option<usize>,
}

impl Bucket {
    /// A custom `endpoint` selects path-style addressing, its absence AWS
    /// virtual-hosted; the agent reads error bodies, since the status alone
    /// cannot tell `SignatureDoesNotMatch` from `NoSuchBucket`.
    pub fn new(
        config: &S3Config,
        cache: CacheControl,
        assets: Fingerprinted,
        access_key: String,
        secret_key: String,
        token: Option<String>,
    ) -> Self {
        let (authority, host, root) = config.endpoint.as_ref().map_or_else(
            || {
                let host = format!("{}.s3.{}.amazonaws.com", config.bucket, config.region());
                (format!("https://{host}"), host, String::new())
            },
            |endpoint| {
                let endpoint = endpoint.trim_end_matches('/');
                let host = endpoint
                    .split_once("://")
                    .map_or(endpoint, |(_, h)| h)
                    .to_owned();
                (endpoint.to_owned(), host, format!("/{}", config.bucket))
            },
        );
        Self {
            agent: crate::remote::Http::agent("deploy", crate::remote::Status::Read),
            name: config.bucket.clone(),
            access_key,
            secret_key,
            token,
            region: config.region().to_owned(),
            prefix: config.prefix.trim_matches('/').to_owned(),
            authority,
            host,
            root,
            cache,
            assets,
            concurrency: config.concurrency,
        }
    }

    /// Every object under the prefix with its ETag, following continuation
    /// tokens to the end.
    ///
    /// Running out of pages is an error and not a short listing: the objects
    /// never mentioned would read as absent and be swept.
    fn objects(&self) -> Result<Inventory> {
        const MAX_PAGES: usize = 10_000;
        let mut out = Inventory::default();
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![("list-type", "2".to_owned())];
            if !self.prefix.is_empty() {
                query.push(("prefix", format!("{}/", self.prefix)));
            }
            if let Some(token) = &token {
                query.push(("continuation-token", token.clone()));
            }
            let body = self.send(
                Method::Get,
                &format!("{}/", self.root),
                &Self::canonical_query(&query),
                &[],
            )?;
            let listing = Listing::parse(&body)?;
            for (key, etag) in listing.objects {
                if Listed::try_from(key.as_str()).is_ok() {
                    out.admit(self.relative(key), etag);
                } else {
                    out.refuse(key);
                }
            }
            match listing.next {
                Some(next) => token = Some(next),
                None => return Ok(out),
            }
        }
        Err(DeployError::Pagination { pages: MAX_PAGES }.into())
    }

    fn object(&self, key: &str) -> String {
        format!(
            "{}/{}",
            self.root,
            Self::encode(&Self::object_key(&self.prefix, key), true)
        )
    }

    /// Strip the configured prefix from a listed object key, so the whole
    /// client speaks one namespace of dist-relative paths.
    fn relative(&self, key: String) -> String {
        if self.prefix.is_empty() {
            return key;
        }
        key.strip_prefix(&format!("{}/", self.prefix))
            .map(str::to_owned)
            .unwrap_or(key)
    }

    /// A signed GET returning the response body; a body that cannot be read is
    /// an error, never an empty listing the sweep would read as a bucket to
    /// empty.
    fn send(&self, method: Method, uri: &str, query: &str, body: &[u8]) -> Result<String> {
        let url = if query.is_empty() {
            self.url(uri)
        } else {
            format!("{}?{query}", self.url(uri))
        };
        let auth = self.authorize(method, uri, query, body);
        let mut response = self
            .signed(self.agent.get(&url), &auth)
            .call()
            .map_err(DeployError::from)?;
        Self::check(method, uri, response.status().as_u16(), &mut response)?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| DeployError::from(e).into())
    }

    /// A signed PUT (with a body) or DELETE (without); ureq types the two
    /// builders differently, so each drives its own call.
    fn write(
        &self,
        method: Method,
        uri: &str,
        body: &[u8],
        headers: &[(&str, &str)],
    ) -> Result<()> {
        let url = self.url(uri);
        let auth = self.authorize(method, uri, "", body);
        let mut response = if method == Method::Delete {
            self.signed(self.agent.delete(&url), &auth).call()
        } else {
            let mut request = self.signed(self.agent.put(&url), &auth);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.send(body)
        }
        .map_err(DeployError::from)?;
        Self::check(method, uri, response.status().as_u16(), &mut response)
    }

    fn signed<Any>(
        &self,
        request: ureq::RequestBuilder<Any>,
        auth: &Authorization,
    ) -> ureq::RequestBuilder<Any> {
        let request = request
            .header("Authorization", &auth.header)
            .header(DATE_HEADER, &auth.timestamp)
            .header(CONTENT_SHA_HEADER, &auth.payload_hash);
        match &self.token {
            Some(token) => request.header(TOKEN_HEADER, token),
            None => request,
        }
    }

    /// `uri` already carries the root and prefix.
    fn url(&self, uri: &str) -> String {
        format!("{}{uri}", self.authority)
    }

    /// The session token is part of the signature and not just a header: one
    /// computed without it is rejected.
    fn authorize(&self, method: Method, uri: &str, query: &str, body: &[u8]) -> Authorization {
        let timestamp = Signer::timestamp(OffsetDateTime::now_utc());
        let payload_hash = Digest::sha256(body);
        let signer = Signer {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            service: SERVICE,
            timestamp: &timestamp,
        };
        let mut headers = vec![(CONTENT_SHA_HEADER, payload_hash.as_str())];
        if let Some(token) = &self.token {
            headers.push((TOKEN_HEADER, token.as_str()));
        }
        let header = signer.sign(&Request {
            method: method.as_str(),
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

    fn check(
        method: Method,
        uri: &str,
        status: u16,
        response: &mut ureq::http::Response<ureq::Body>,
    ) -> Result<()> {
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body = response.body_mut().read_to_string().unwrap_or_default();
        Err(DeployError::request(method, uri, status, &body).into())
    }
}

impl Store for Bucket {
    /// Every object is its own request, so a bucket takes as many at once as
    /// the site asks for.
    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }

    /// A single-part upload's ETag is the hex MD5 of its bytes.
    fn digest(&self, bytes: &[u8]) -> String {
        Self::etag(bytes)
    }

    fn list(&self, ui: &Ui) -> Result<Digests> {
        Ok(self.objects()?.report(ui, &self.target()))
    }

    /// Content type comes from the extension, cache policy from whether the
    /// name is a hash.
    fn upload(&self, key: &str, body: &[u8]) -> Result<()> {
        let content_type = Mime::of(key).header();
        let mut headers = vec![("Content-Type", content_type.as_str())];
        if let Some(cache) = self
            .cache
            .header(key, &self.assets.prefix, self.assets.hashed)
        {
            headers.push(("Cache-Control", cache));
        }
        self.write(Method::Put, &self.object(key), body, &headers)
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.write(Method::Delete, &self.object(key), &[], &[])
    }

    fn target(&self) -> String {
        self.name.clone()
    }
}

const TOKEN_HEADER: &str = "x-amz-security-token";

/// Required on every signed request.
const CONTENT_SHA_HEADER: &str = "x-amz-content-sha256";

/// The credential scope's service name; every S3-compatible host expects `s3`,
/// whatever it calls itself.
const SERVICE: &str = "s3";

struct Authorization {
    header: String,
    timestamp: String,
    payload_hash: String,
}

/// Keys are exactly as the bucket named them; deciding which this client may
/// act on belongs to [`Bucket::objects`], where a refusal can be recorded.
struct Listing {
    objects: Vec<(String, String)>,
    next: Option<String>,
}

/// Wire-format helpers: key normalization, encoding, and signing values.
impl Bucket {
    /// Forward-slashed, no leading slash, prefix folded in.
    fn object_key(prefix: &str, path: &str) -> String {
        let path = Slashed(Path::new(path)).to_string();
        let path = path.trim_start_matches('/');
        if prefix.is_empty() {
            path.to_owned()
        } else {
            format!("{prefix}/{path}")
        }
    }

    /// Percent-encode per the S3 signing rules; `keep_slash` is set for a path
    /// and clear for a query component, which encodes its separators too.
    fn encode(value: &str, keep_slash: bool) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut out = String::with_capacity(value.len());
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(char::from(byte));
                }
                b'/' if keep_slash => out.push('/'),
                _ => {
                    out.push('%');
                    out.push(char::from(HEX[usize::from(byte >> 4)]));
                    out.push(char::from(HEX[usize::from(byte & 0x0f)]));
                }
            }
        }
        out
    }

    /// Each name and value URI-encoded, sorted by name.
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

    /// The ETag S3 assigns a single-part upload: the lowercase hex MD5.
    fn etag(bytes: &[u8]) -> String {
        Digest::hex(&Md5::digest(bytes))
    }
}

impl Listing {
    /// Parse a ListObjectsV2 XML response into its objects and continuation token.
    fn parse(xml: &str) -> Result<Self> {
        let document = roxmltree::Document::parse(xml).map_err(DeployError::from)?;
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
                let key = text(node, "Key")?;
                let etag = Self::unquote(&text(node, "ETag")?).to_owned();
                Some((key, etag))
            })
            .collect();
        let next = document
            .descendants()
            .find(|node| node.has_tag_name("NextContinuationToken"))
            .and_then(|node| node.text())
            .map(str::to_owned);
        Ok(Self { objects, next })
    }

    /// Strip the surrounding quotes S3 wraps an ETag in.
    fn unquote(etag: &str) -> &str {
        etag.trim_matches('"')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both are spliced into `<bucket>.s3.<region>.amazonaws.com`, and the
    /// request carries a usable AWS credential: a `region` of `evil.example/`
    /// sent it to `evil.example`.
    #[test]
    fn a_bucket_or_region_that_is_not_a_name_is_refused() {
        for (bucket, region) in [
            ("my-site", Some("evil.example/")),
            ("my-site/../other", None),
            ("my site", None),
            ("my-site", Some("us-east-1:443")),
            ("my-site", Some("us-east-1?x")),
        ] {
            let mut cfg = config(None, "");
            cfg.bucket = bucket.into();
            cfg.region = region.map(String::from);
            assert!(S3::check(&cfg).is_err(), "{bucket} / {region:?}");
        }

        let mut ok = config(None, "");
        ok.region = Some("eu-west-3".into());
        assert!(S3::check(&ok).is_ok());
    }

    fn config(endpoint: Option<&str>, prefix: &str) -> S3Config {
        S3Config {
            bucket: "my-site".into(),
            endpoint: endpoint.map(String::from),
            region: None,
            prefix: prefix.into(),
            delete: true,
            concurrency: None,
        }
    }

    fn fingerprinted() -> Fingerprinted {
        Fingerprinted {
            prefix: "assets".into(),
            hashed: true,
        }
    }

    fn bucket(endpoint: Option<&str>, prefix: &str) -> Bucket {
        Bucket::new(
            &config(endpoint, prefix),
            CacheControl::default(),
            fingerprinted(),
            "AKID".into(),
            "secret".into(),
            None,
        )
    }

    #[test]
    fn only_content_addressed_keys_are_immutable() {
        let mut policy = CacheControl::default();
        assert_eq!(policy.header("assets/app.abc.css", "assets", true), None);

        policy.enabled = true;
        policy.immutable = "immutable".into();
        policy.default = "revalidate".into();
        assert_eq!(
            policy.header("assets/app.abc.css", "assets", true),
            Some("immutable")
        );
        assert_eq!(
            policy.header("index.html", "assets", true),
            Some("revalidate")
        );
        assert_eq!(
            policy.header("/assets/app.abc.css", "assets", true),
            Some("immutable")
        );
        assert_eq!(
            policy.header("assets/app.css", "assets", false),
            Some("revalidate")
        );
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
        assert_eq!(b.relative("other/a.html".into()), "other/a.html");
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
    fn listing_keeps_the_keys_the_bucket_named() {
        let xml = r#"<ListBucketResult>
              <Contents><Key>a.html</Key><ETag>"abc"</ETag></Contents>
              <Contents><Key>posts//a.html</Key><ETag>"def"</ETag></Contents>
              <Contents><Key>../etc/passwd</Key><ETag>"ghi"</ETag></Contents>
            </ListBucketResult>"#;
        let listing = Listing::parse(xml).unwrap();
        assert_eq!(listing.objects.len(), 3);
        assert!(Listed::try_from("posts//a.html").is_err());
        assert!(Listed::try_from("../etc/passwd").is_err());
    }

    #[test]
    fn an_unnamed_bucket_is_refused() {
        assert!(S3::check(&config(None, "")).is_ok());
        assert!(S3::check(&S3Config::default()).is_err());
    }

    #[test]
    fn a_plaintext_endpoint_is_reported() {
        assert!(S3::plaintext(None).is_none());
        assert!(S3::plaintext(Some("https://acct.r2.cloudflarestorage.com")).is_none());
        let warning = S3::plaintext(Some("http://localhost:9000")).expect("warned");
        assert_eq!(warning.url, "http://localhost:9000");
    }

    #[test]
    fn listing_without_token_ends() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";
        let listing = Listing::parse(xml).unwrap();
        assert!(listing.objects.is_empty());
        assert_eq!(listing.next, None);
    }
}
