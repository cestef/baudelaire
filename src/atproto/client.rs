//! A minimal, blocking XRPC client for the `com.atproto.*` methods a publisher
//! needs: authenticate, upload a blob, put/list/delete records. Built on `ureq`
//! (blocking, rustls) so publishing needs no async runtime; a [`Session`]'s
//! `ureq::Agent` is `Send + Sync`, so a caller may drive calls from a rayon pool.

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::PublishError;
use crate::mime::Mime;

use super::id::{Did, Nsid, Rkey};

/// A blob reference returned by `uploadBlob`, embedded verbatim into the owning
/// record. Opaque: its internal shape (`$type`, `ref`, `mimeType`, `size`) is
/// the PDS's to define, so it round-trips unmodified. Serializes transparently.
#[derive(Debug, Clone, Serialize)]
pub struct Blob(Value);

/// An authenticated session against a single PDS host.
pub struct Session {
    agent: ureq::Agent,
    /// The PDS/entryway base, e.g. `https://bsky.social`, without a trailing `/`.
    host: String,
    /// The repository DID that authenticated — the source of truth for identity,
    /// overriding any configured guess.
    did: Did,
    /// The bearer access token.
    access: String,
}

impl Session {
    /// Authenticate to `host` with a handle (or DID) and app password.
    pub fn login(host: &str, identifier: &str, password: &str) -> Result<Self, PublishError> {
        // The PDS returns 4xx/5xx as ordinary responses, not transport errors,
        // so the XRPC error body can be read and surfaced.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let host = host.trim_end_matches('/').to_owned();
        let url = format!("{host}/xrpc/com.atproto.server.createSession");
        let body = json!({ "identifier": identifier, "password": password });
        let mut resp = agent.post(&url).send_json(&body)?;
        let value: Value = resp.json("com.atproto.server.createSession")?;
        Ok(Self {
            agent,
            host,
            did: Did::new(value.field("did")?),
            access: value.field("accessJwt")?,
        })
    }

    /// The authenticated repository DID.
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Upload `bytes` (of type `mime`) as a blob and return its reference.
    pub fn upload_blob(&self, bytes: &[u8], mime: Mime) -> Result<Blob, PublishError> {
        const NSID: &str = "com.atproto.repo.uploadBlob";
        let mut resp = self
            .agent
            .post(self.xrpc(NSID))
            .header("Authorization", self.bearer())
            .header("Content-Type", mime.to_string())
            .send(bytes)?;
        let value: Value = resp.json(NSID)?;
        value.get("blob").cloned().map(Blob).ok_or_else(|| {
            PublishError::xrpc(NSID, resp.status().as_u16(), "response had no `blob`")
        })
    }

    /// Create or replace a record at `collection/rkey`.
    pub fn put_record(
        &self,
        collection: Nsid,
        rkey: &Rkey,
        record: &impl Serialize,
    ) -> Result<(), PublishError> {
        const NSID: &str = "com.atproto.repo.putRecord";
        self.post(
            NSID,
            &json!({
                "repo": self.did.as_str(),
                "collection": collection.as_str(),
                "rkey": rkey.as_str(),
                "record": record,
            }),
        )
    }

    /// Delete the record at `collection/rkey`.
    pub fn delete_record(&self, collection: Nsid, rkey: &Rkey) -> Result<(), PublishError> {
        const NSID: &str = "com.atproto.repo.deleteRecord";
        self.post(
            NSID,
            &json!({
                "repo": self.did.as_str(),
                "collection": collection.as_str(),
                "rkey": rkey.as_str(),
            }),
        )
    }

    /// Every record key currently in `collection`, following pagination — the
    /// remote source of truth a publisher diffs against, so a removed page's
    /// record is deleted and nothing is orphaned.
    pub fn list_rkeys(&self, collection: Nsid) -> Result<Vec<Rkey>, PublishError> {
        const NSID: &str = "com.atproto.repo.listRecords";
        let mut rkeys = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut req = self
                .agent
                .get(self.xrpc(NSID))
                .header("Authorization", self.bearer())
                .query("repo", self.did.as_str())
                .query("collection", collection.as_str())
                .query("limit", "100");
            if let Some(cursor) = &cursor {
                req = req.query("cursor", cursor);
            }
            let mut resp = req.call()?;
            let value: Value = resp.json(NSID)?;
            for record in value
                .get("records")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(rkey) = record
                    .get("uri")
                    .and_then(Value::as_str)
                    .and_then(|uri| uri.rsplit('/').next())
                {
                    rkeys.push(Rkey::parsed(rkey));
                }
            }
            match value.get("cursor").and_then(Value::as_str) {
                Some(next) if !next.is_empty() => cursor = Some(next.to_owned()),
                _ => break,
            }
        }
        Ok(rkeys)
    }

    /// POST a JSON `body` to `nsid` with bearer auth, discarding the response —
    /// the shared spine of the record-mutating calls (put/delete).
    fn post(&self, nsid: &str, body: &Value) -> Result<(), PublishError> {
        let mut resp = self
            .agent
            .post(self.xrpc(nsid))
            .header("Authorization", self.bearer())
            .send_json(body)?;
        resp.json::<Value>(nsid).map(drop)
    }

    fn xrpc(&self, nsid: &str) -> String {
        format!("{}/xrpc/{nsid}", self.host)
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.access)
    }
}

/// Read an XRPC response body as `T`, turning a non-2xx status into a
/// [`PublishError`] carrying the PDS's own error body.
trait ResponseExt {
    fn json<T: serde::de::DeserializeOwned>(&mut self, nsid: &str) -> Result<T, PublishError>;
}

impl ResponseExt for ureq::http::Response<ureq::Body> {
    fn json<T: serde::de::DeserializeOwned>(&mut self, nsid: &str) -> Result<T, PublishError> {
        let status = self.status().as_u16();
        if !(200..300).contains(&status) {
            let message = self.body_mut().read_to_string().unwrap_or_default();
            return Err(PublishError::xrpc(nsid, status, message));
        }
        Ok(self.body_mut().read_json::<T>()?)
    }
}

/// A required string field of a JSON object, else an auth error naming it.
trait ValueExt {
    fn field(&self, key: &str) -> Result<String, PublishError>;
}

impl ValueExt for Value {
    fn field(&self, key: &str) -> Result<String, PublishError> {
        self.get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| PublishError::auth(format!("session response had no `{key}`")))
    }
}
